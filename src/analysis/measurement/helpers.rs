use super::dwarf::{self, Subprogram, Subroutine};
use anyhow::Result;
use probe_rs::Core;

pub const BKPT_UNKNOWN_NAME: &str = "<unknown>";

/// Tries to read the name of the current task from the Subprograms.
///
/// * `core` - A connected probe-rs _core_
/// * `subprograms` - A list of the all the subprograms of the running program
pub fn read_breakpoint_task_name(core: &mut Core, subprograms: &Vec<Subprogram>) -> Result<String> {
    let optimal = get_current_task_from_lr(core, subprograms)?;

    let name = match optimal {
        Some(s) => s.name,
        None => BKPT_UNKNOWN_NAME.to_string(),
    };
    Ok(name)
}

/// Returns the current vcell (if any) via the link register.
///
/// * `core` - A connected probe-rs _core_
/// * `vcells` - A list of all the vcell readings in the program
pub fn get_current_vcell_from_lr(
    core: &mut Core,
    vcells: &Vec<Subroutine>,
) -> Result<Option<Subroutine>> {
    // We read the link register to check where to return after the breakpoint
    let lr = core.registers().return_address();
    // Decrement with 1 because otherwise it will point outside the vcell reading
    let lr_val = core.read_core_reg(lr)? - 1;

    let in_range = dwarf::get_subroutines_address_in_range(&vcells, lr_val as u64)?;
    let optimal = dwarf::get_shortest_range_subroutine(&in_range)?;

    Ok(optimal)
}

/// Returns the current task (if any) via the link register. Works only if called
/// from within a breakpoint.
///
/// * `core` - A connected probe-rs _core_
/// * `subprograms` - A list of the all the subprograms of the running program
pub fn get_current_task_from_lr(
    core: &mut Core,
    subprograms: &Vec<Subprogram>,
) -> Result<Option<Subprogram>> {
    // We read the link register to check where to return after the breakpoint
    let lr = core.registers().return_address();
    // This returns a PC inside the task we want to find the name for
    let lr_val = core.read_core_reg(lr)?;

    let in_range = dwarf::get_subprograms_address_in_range(subprograms, lr_val as u64)?;
    let optimal = dwarf::get_shortest_range_subprogram(&in_range)?;

    Ok(optimal)
}

/// Tries to read the name of the resources that is currently locked from the Subroutines.
///
/// * `core` - A connected probe-rs _core_
/// * `resource_locks` - A lsit of all resource locks
pub fn read_breakpoint_lock_name(
    core: &mut Core,
    resource_locks: &Vec<Subroutine>,
) -> Result<String> {
    let optimal = get_current_resource_lock(core, resource_locks)?;

    let name = match optimal {
        Some(s) => s.name,
        None => BKPT_UNKNOWN_NAME.to_string(),
    };
    Ok(name)
}

/// Returns the current resource lock we're inside via the link register. Works only if called
/// from within a breakpoint.
///
/// * `core` - A connected probe-rs _core_
/// * `resource_locks` - A lsit of all resource locks
pub fn get_current_resource_lock(
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
