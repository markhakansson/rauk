use super::breakpoints::{Breakpoint, OtherBreakpoint};
use super::dwarf::{self, ObjectLocationMap, Subprogram, Subroutine};
use crate::utils::klee::get_vcell_ktestobjects;
use crate::utils::probe as core_utils;
use anyhow::{anyhow, Context, Result};
use ktest_parser::KTest;
use probe_rs::{Core, CoreRegisterAddress, MemoryInterface};

type ObjectName = String;
type CycleCount = u32;
/// Result of measuring on hardware. Containing the Breakpoint type and the name of the object
/// (such as a Task name or resources name) and the cycle count at that breakpoint.
pub type MeasurementResult = (Breakpoint, ObjectName, CycleCount);

const HALT_TIMEOUT_SECONDS: u64 = 10;
const BKPT_UNKNOWN_NAME: &str = "<unknown>";

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
/// between the ReplayStart breakpoints and return them as a list.
///
/// * `core` - A connected probe-rs _core_
/// * `subprograms` - A list of all subprograms of RTIC tasks
/// * `resource_locks` - A list of all RTIC resource locks
/// * `vcells` - A list of all hardware peripheral accesses
fn read_breakpoints(
    core: &mut Core,
    subprograms: &Vec<Subprogram>,
    resource_locks: &Vec<Subroutine>,
    vcells: &Vec<Subroutine>,
    ktest: &KTest,
) -> Result<Vec<MeasurementResult>> {
    let mut stack: Vec<MeasurementResult> = Vec::new();
    let name = BKPT_UNKNOWN_NAME.to_string();

    // For HW accesses
    let mut current_hw_bkpt: u32 = 0;
    let mut vcell_stack: Vec<Subroutine> = Vec::new();
    let mut test_stack = get_vcell_ktestobjects(ktest);
    test_stack.reverse();

    loop {
        core_utils::run(core).context("Could not continue from replay start")?;
        core.wait_for_core_halted(std::time::Duration::from_secs(HALT_TIMEOUT_SECONDS))?;
        if core_utils::current_pc(core)? == current_hw_bkpt && current_hw_bkpt != 0 {
            // Clear current hw breakpoint. Overwrite r0 with KTestObject value
            core.clear_hw_breakpoint(current_hw_bkpt)?;
            current_hw_bkpt = 0;
            if let Some(test) = test_stack.pop() {
                if test.num_bytes == 4 {
                    let bytes: [u8; 4] =
                        [test.bytes[0], test.bytes[1], test.bytes[2], test.bytes[3]];
                    let data = u32::from_le_bytes(bytes);
                    core.write_core_reg(CoreRegisterAddress(0), data)?;
                }
            }
            // If vcell stack is not empty. Write it and then set new hw breakpoint;
            if let Some(vcell) = vcell_stack.pop() {
                current_hw_bkpt = vcell.high_pc as u32;
                core.set_hw_breakpoint(current_hw_bkpt)?;
            }
        } else if !core_utils::breakpoint_at_pc(core)? {
            return Err(anyhow!(
                "Core halted, but not due to breakpoint. Can't continue with analysis."
            ));
        } else {
            // Read breakpoint immediate value
            let imm = Breakpoint::from(core_utils::read_breakpoint_value(core)?);
            match imm {
                // On ReplayStart the loop is complete
                Breakpoint::Other(OtherBreakpoint::ReplayStart) => break,
                // Save the name and continue to the next loop iteration
                Breakpoint::Other(OtherBreakpoint::InsideTask) => {
                    let name = read_breakpoint_task_name(core, &subprograms)?;

                    // DONT SET VCELL STACK IF IT IS NOT EMPTY???
                    // Get all vcells in range of this lock and update vcell_stack
                    // Should not set this twice?
                    if vcell_stack.is_empty() {
                        let current_task = get_current_task(core, subprograms)?.unwrap();
                        let subs =
                            dwarf::get_subprograms_with_name(subprograms, &current_task.name);
                        for sub in subs {
                            let mut stack =
                                dwarf::get_subroutines_in_range(&vcells, sub.low_pc, sub.high_pc)?;
                            vcell_stack.append(&mut stack);
                        }
                        vcell_stack.dedup();

                        // Sort by order
                        vcell_stack.sort_by(|b, a| a.low_pc.cmp(&b.low_pc));

                        // Set hw breakpoint at first vcell
                        if let Some(vcell) = vcell_stack.pop() {
                            current_hw_bkpt = vcell.high_pc as u32;
                            core.set_hw_breakpoint(current_hw_bkpt)?;
                        }
                    }

                    let (b, _, u) = stack.pop().unwrap();
                    stack.push((b, name, u));

                    continue;
                }
                // Save the name and continue to the next loop iteration
                Breakpoint::Other(OtherBreakpoint::InsideLock) => {
                    let name = read_breakpoint_lock_name(core, &resource_locks)?;
                    let (b, _, u) = stack.pop().unwrap();
                    stack.push((b, name, u));

                    continue;
                }
                // Ignore everything else for now
                _ => (),
            }

            let cyccnt = core_utils::read_cycle_counter(core)?;
            stack.push((imm, name.clone(), cyccnt));
        }
    }

    Ok(stack)
}

fn get_current_task(core: &mut Core, subprograms: &Vec<Subprogram>) -> Result<Option<Subprogram>> {
    // We read the link register to check where to return after the breakpoint
    let lr = core.registers().return_address();
    // This returns a PC inside the task we want to find the name for
    let lr_val = core.read_core_reg(lr)?;

    let in_range = dwarf::get_subprograms_address_in_range(subprograms, lr_val as u64)?;
    let optimal = dwarf::get_shortest_range_subprogram(&in_range)?;

    Ok(optimal)
}

/// Returns the current resource lock we're inside
fn get_current_resource_lock(
    core: &mut Core,
    resource_locks: &Vec<Subroutine>,
) -> Result<Option<Subroutine>> {
    // We read the link register to check where to return after the breakpoint
    let lr = core.registers().return_address();
    // This returns a PC inside the task we want to find the name for
    let lr_val = core.read_core_reg(lr)?;

    let in_range = dwarf::get_subroutines_address_in_range(resource_locks, lr_val as u64)?;
    let optimal = dwarf::get_shortest_range_subroutine(&in_range)?;

    Ok(optimal)
}
/// Tries to read the name of the current task from the Subprograms
fn read_breakpoint_task_name(core: &mut Core, subprograms: &Vec<Subprogram>) -> Result<String> {
    // We read the link register to check where to return after the breakpoint
    let lr = core.registers().return_address();
    // This returns a PC inside the task we want to find the name for
    let lr_val = core.read_core_reg(lr)?;

    let in_range = dwarf::get_subprograms_address_in_range(subprograms, lr_val as u64)?;
    let optimal = dwarf::get_shortest_range_subprogram(&in_range)?;

    let name = match optimal {
        Some(s) => s.name,
        None => BKPT_UNKNOWN_NAME.to_string(),
    };
    Ok(name)
}

/// Tries to read the name of the resources that is currently locked from the Subroutines
fn read_breakpoint_lock_name(core: &mut Core, resource_locks: &Vec<Subroutine>) -> Result<String> {
    // We read the link register to check where to return after the breakpoint
    let lr = core.registers().return_address();
    // This returns a PC inside the lock we want to find the name for
    let lr_val = core.read_core_reg(lr)?;

    let in_range = dwarf::get_subroutines_address_in_range(resource_locks, lr_val as u64)?;
    let optimal = dwarf::get_shortest_range_subroutine(&in_range)?;

    let name = match optimal {
        Some(s) => s.name,
        None => BKPT_UNKNOWN_NAME.to_string(),
    };
    Ok(name)
}

/// Runs to where the replay harness starts.
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

fn _write_vcell_object(core: &mut Core, ktest: &KTest, vcell: &Subroutine) {}
