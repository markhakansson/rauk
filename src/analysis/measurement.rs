use super::breakpoints::{Breakpoint, OtherBreakpoint};
use super::dwarf::{self, ObjectLocationMap, Subprogram, Subroutine};
use crate::utils::probe as core_utils;
use anyhow::{anyhow, Context, Result};
use ktest_parser::KTest;
use probe_rs::{Core, MemoryInterface};

/// Result of measuring on hardware. (Breakpoint, Name, Program counter).
pub type MeasurementResult = (Breakpoint, String, u32);

const HALT_TIMEOUT_SECONDS: u64 = 5;
const BKPT_UNKNOWN_NAME: &str = "<unknown>";

/// Read all breakpoints and the cycle counter at their positions
/// between the ReplayStart breakpoints and return them as a list.
///
/// * `core` - A connected probe-rs _core_
/// * `subprograms` - A list of all subprograms of RTIC tasks
/// * `resource_locks` - A list of all RTIC resource locks
pub fn read_breakpoints(
    core: &mut Core,
    subprograms: &Vec<Subprogram>,
    resource_locks: &Vec<Subroutine>,
) -> Result<Vec<MeasurementResult>> {
    let mut stack: Vec<MeasurementResult> = Vec::new();
    let name = BKPT_UNKNOWN_NAME.to_string();

    loop {
        core_utils::run(core).context("Could not continue from replay start")?;
        core.wait_for_core_halted(std::time::Duration::from_secs(HALT_TIMEOUT_SECONDS))?;
        if !core_utils::breakpoint_at_pc(core)? {
            return Err(anyhow!(
                "Core halted, but not due to breakpoint. Can't continue with analysis."
            ));
        }

        // Read breakpoint immediate value
        let imm = Breakpoint::from(core_utils::read_breakpoint_value(core)?);
        match imm {
            // On ReplayStart the loop is complete
            Breakpoint::Other(OtherBreakpoint::ReplayStart) => break,
            // Save the name and continue to the next loop iteration
            Breakpoint::Other(OtherBreakpoint::InsideTask) => {
                let name = read_breakpoint_task_name(core, &subprograms)?;
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

    Ok(stack)
}

/// Tries to read the name of the current task from the Subprograms
fn read_breakpoint_task_name(core: &mut Core, subprograms: &Vec<Subprogram>) -> Result<String> {
    // We read the link register to check where to return after the breakpoint
    let lr = core.registers().return_address();
    // This returns a PC inside the task we want to find the name for
    let lr_val = core.read_core_reg(lr)?;

    let in_range = dwarf::get_subprograms_in_range(subprograms, lr_val as u64)?;
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

    let in_range = dwarf::get_subroutines_in_range(resource_locks, lr_val as u64)?;
    let optimal = dwarf::get_shortest_range_subroutine(&in_range)?;

    let name = match optimal {
        Some(s) => s.name,
        None => BKPT_UNKNOWN_NAME.to_string(),
    };
    Ok(name)
}

/// Runs to where the replay harness starts.
pub fn run_to_replay_start(core: &mut Core) -> Result<()> {
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
pub fn write_replay_objects(
    core: &mut Core,
    ktest: &KTest,
    locations: &ObjectLocationMap,
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
