mod helpers;

use super::breakpoints::{Breakpoint, OtherBreakpoint};
use super::dwarf::{self, ObjectLocationMap, Subprogram, Subroutine};
use crate::utils::core as core_utils;
use crate::utils::klee::get_vcell_ktestobjects;
use anyhow::{anyhow, Context, Result};
use helpers::*;
use ktest_parser::{KTest, KTestObject};
use probe_rs::{Core, CoreRegisterAddress, MemoryInterface};

type ObjectName = String;
type CycleCount = u32;
/// Result of measuring on hardware. Containing the Breakpoint type and the name of the object
/// (such as a Task name or resources name) and the cycle count at that breakpoint.
pub type MeasurementResult = (Breakpoint, ObjectName, CycleCount);

const HALT_TIMEOUT_SECONDS: u64 = 10;

/// Runs the replay harness and measures the clock cycles.
pub fn measure_replay_harness(
    core: &mut Core,
    ktests: &Vec<KTest>,
    resource_addresses: &ObjectLocationMap,
    subprograms: &Vec<Subprogram>,
    resource_locks: &Vec<Subroutine>,
    vcells: &mut Vec<Subroutine>,
) -> Result<Vec<Vec<MeasurementResult>>> {
    let mut measurements: Vec<Vec<MeasurementResult>> = Vec::new();

    // Measurement on hardware
    for ktest in ktests {
        // Continue until reaching BKPT 255 (replaystart)
        run_to_replay_start(core).context("Could not continue to replay start")?;
        write_replay_objects(core, &resource_addresses, &ktest)
            .with_context(|| format!("Could not write to memory with KTest: {:?}", &ktest))?;
        let bkpts = read_breakpoints(core, &subprograms, &resource_locks, vcells, &ktest)?;
        measurements.push(bkpts);
    }

    Ok(measurements)
}

/// Read all breakpoints and the cycle counter at their positions
/// from the start of a ReplayStart breakpoint until the next ReplayStart breakpoint.
/// Return the measurement result as a list.
///
/// * `core` - A connected probe-rs _core_
/// * `subprograms` - A list of all subprograms of RTIC tasks
/// * `resource_locks` - A list of all RTIC resource locks
/// * `vcells` - A list of all hardware peripheral accesses
/// * `ktest` - The test to replay
fn read_breakpoints(
    core: &mut Core,
    subprograms: &Vec<Subprogram>,
    resource_locks: &Vec<Subroutine>,
    vcells: &Vec<Subroutine>,
    ktest: &KTest,
) -> Result<Vec<MeasurementResult>> {
    let mut measurements: Vec<MeasurementResult> = Vec::new();
    let name = BKPT_UNKNOWN_NAME.to_string();

    // For HW accesses
    let mut current_hw_bkpt: u32 = 0;
    let mut test_stack = get_vcell_ktestobjects(ktest);
    test_stack.reverse();

    loop {
        core_utils::run(core).context("Could not continue from replay start")?;
        core.wait_for_core_halted(std::time::Duration::from_secs(HALT_TIMEOUT_SECONDS))?;

        if core_utils::current_pc(core)? == current_hw_bkpt && current_hw_bkpt != 0 {
            // Clear current hw breakpoint. Overwrite r0 with KTestObject value
            core.clear_hw_breakpoint(current_hw_bkpt)?;
            current_hw_bkpt = 0;
            // It is assumed vcells occur in order so just pop the first test
            if let Some(test) = test_stack.pop() {
                write_vcell_test_to_register(core, &test)?;
            }
        } else if !core_utils::breakpoint_at_pc(core)? {
            return Err(anyhow!(
                "Core halted, but not due to breakpoint. Can't continue with analysis."
            ));
        } else {
            let bkpt = Breakpoint::from(core_utils::read_breakpoint_value(core)?);
            match bkpt {
                // On ReplayStart the loop is complete
                Breakpoint::Other(OtherBreakpoint::ReplayStart) => break,
                // Save the name and continue to the next loop iteration
                Breakpoint::Other(OtherBreakpoint::InsideTask) => {
                    let name = read_breakpoint_task_name(core, &subprograms)?;
                    let (b, _, u) = measurements.pop().unwrap();
                    measurements.push((b, name, u));

                    continue;
                }
                // Save the name and continue to the next loop iteration
                Breakpoint::Other(OtherBreakpoint::InsideLock) => {
                    let name = read_breakpoint_lock_name(core, &resource_locks)?;
                    let (b, _, u) = measurements.pop().unwrap();
                    measurements.push((b, name, u));

                    continue;
                }
                // If inside a vcell set hardware breakpoint before exiting vcell then continue
                Breakpoint::Other(OtherBreakpoint::InsideLockClosure) => {
                    // Get all vcells in range of this lock and update vcell_stack
                    if let Some(current_vcell) = get_current_vcell_from_lr(core, &vcells)? {
                        // Need to increment with 2 here. Because the last instruction of the
                        // vcell function will overwrite `r0` and we need to step over it.
                        // Then overwrite `r0` ourselves!
                        current_hw_bkpt = current_vcell.high_pc as u32 + 2;
                        core.set_hw_breakpoint(current_hw_bkpt)?;
                    }

                    continue;
                }
                // Ignore everything else for now
                _ => (),
            }

            // Save the result onto the stack
            let cyccnt = core_utils::read_cycle_counter(core)?;
            measurements.push((bkpt, name.clone(), cyccnt));
        }
    }

    Ok(measurements)
}

/// Runs to where the replay harness starts. Also runs past any other breakpoints
/// on the way, should there be any.
///
/// * `core` - A connected probe-rs _core_
fn run_to_replay_start(core: &mut Core) -> Result<()> {
    // Wait for core to halt on a breakpoint. If it doesn't
    // something is wrong.
    core.wait_for_core_halted(std::time::Duration::from_secs(HALT_TIMEOUT_SECONDS))?;
    loop {
        let imm = core_utils::read_breakpoint_value(core)?;
        // Ready to analyze when reaching this breakpoint
        if imm == OtherBreakpoint::ReplayStart as u8 {
            break;
        }
        // Should there be other breakpoints we continue past them
        core_utils::run(core)?;
    }
    Ok(())
}

/// Writes the replay contents of the KTEST file to the objects memory addresses.
/// If no memory address was found for the specific KTEST, it will ignore writing
/// anything to it.
///
/// * `core` - A connected probe-rs _core_
/// * `locations` - A map of RTIC resource names and their memory addresses
/// * `ktest` - The test vector to write to its corresponding memory address
fn write_replay_objects(
    core: &mut Core,
    locations: &ObjectLocationMap,
    ktest: &KTest,
) -> Result<()> {
    for test in &ktest.objects {
        let location = locations.get(&test.name);
        match location {
            Some(addr) => {
                let a = addr.unwrap() as u32;
                let slice = test.bytes.as_slice();
                core.write_8(a, slice)?;
            }
            None => {
                // Should log a warning here instead
                // return Err(anyhow!(
                //     "Address was not found for KTestObject: {:?}",
                //     &test
                // ));
                ()
            }
        }
    }
    Ok(())
}

/// Writes a test vector for a vcell reading to the return register.
///
/// * `core` - A connected probe-rs _core_
/// * `test` - The test vector
fn write_vcell_test_to_register(core: &mut Core, test: &KTestObject) -> Result<()> {
    if test.num_bytes == 4 {
        let bytes: [u8; 4] = [test.bytes[0], test.bytes[1], test.bytes[2], test.bytes[3]];
        let data = u32::from_le_bytes(bytes);
        core.write_core_reg(CoreRegisterAddress(0), data)?;
    } else {
        // Log a warning here
    }
    Ok(())
}