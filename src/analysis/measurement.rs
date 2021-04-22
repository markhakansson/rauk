use super::dwarf::{self, Subprogram, Subroutine};
use crate::utils::probe as core_utils;
use anyhow::{anyhow, Context, Result};
use probe_rs::Core;

const HALT_TIMEOUT_SECONDS: u64 = 5;
const BKPT_UNKNOWN_NAME: &str = "<unknown>";

/// Information about the breakpoint type for RAUK analysis
#[derive(Debug, Clone, PartialEq)]
pub enum Breakpoint {
    Other(OtherBreakpoint),
    Entry(EntryBreakpoint),
    Exit(ExitBreakpoint),
}

impl Breakpoint {
    pub fn is_exit(&self) -> bool {
        match self {
            Breakpoint::Exit(_) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EntryBreakpoint {
    HardwareTaskStart = 2,
    ResourceLockStart = 3,
    SoftwareTaskStart = 4,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExitBreakpoint {
    SoftwareTaskEnd = 251,
    ResourceLockEnd = 252,
    HardwareTaskEnd = 253,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OtherBreakpoint {
    Default = 0,
    InsideTask = 1,
    Invalid = 100,
    InsideLock = 254,
    ReplayStart = 255,
}

impl From<u8> for Breakpoint {
    fn from(u: u8) -> Breakpoint {
        match u {
            0 => Breakpoint::Other(OtherBreakpoint::Default),
            1 => Breakpoint::Other(OtherBreakpoint::InsideTask),
            2 => Breakpoint::Entry(EntryBreakpoint::HardwareTaskStart),
            3 => Breakpoint::Entry(EntryBreakpoint::ResourceLockStart),
            4 => Breakpoint::Entry(EntryBreakpoint::SoftwareTaskStart),
            251 => Breakpoint::Exit(ExitBreakpoint::SoftwareTaskEnd),
            252 => Breakpoint::Exit(ExitBreakpoint::ResourceLockEnd),
            253 => Breakpoint::Exit(ExitBreakpoint::HardwareTaskEnd),
            254 => Breakpoint::Other(OtherBreakpoint::InsideLock),
            255 => Breakpoint::Other(OtherBreakpoint::ReplayStart),
            _ => Breakpoint::Other(OtherBreakpoint::Invalid),
        }
    }
}

/// Read all breakpoints and the cycle counter at their positions
/// between the ReplayStart breakpoints and return them as a list
pub fn read_breakpoints(
    core: &mut Core,
    subprograms: &Vec<Subprogram>,
    resource_locks: &Vec<Subroutine>,
) -> Result<Vec<(Breakpoint, String, u32)>> {
    let mut stack: Vec<(Breakpoint, String, u32)> = Vec::new();
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
