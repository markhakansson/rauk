use super::breakpoints::{Breakpoint, OtherBreakpoint};
use super::dwarf::{self, ObjectLocationMap, Subprogram, Subroutine};
use super::klee::get_vcell_ktestobjects;
use super::AppInfo;
use crate::utils::core;
use anyhow::{anyhow, Context, Result};
use ktest_parser::{KTest, KTestObject};
use probe_rs::{Core, CoreRegisterAddress, MemoryInterface};

pub const BKPT_UNKNOWN_NAME: &str = "<unknown>";
const DEFAULT_HALT_TIMEOUT_SECONDS: u64 = 10;

type ObjectName = String;
type CycleCount = u32;
/// Result of measuring on hardware. Containing the Breakpoint type and the name of the object
/// (such as a Task name or resources name) and the cycle count at that breakpoint.
pub type MeasurementResult = (Breakpoint, ObjectName, CycleCount);

enum LoopAction {
    Break,
    Continue,
    Nothing,
}

/// Runs the replay harness and measures the clock cycles.
///
/// * `core` - A connected probe-rs _core_
/// * `ktests` - The generated test vectors
/// * `app` - Relevant information of the replay binary
pub(super) fn measure_replay_harness(
    core: &mut Core,
    ktests: &Vec<KTest>,
    app: &AppInfo,
) -> Result<Vec<Vec<MeasurementResult>>> {
    let mut measurements: Vec<Vec<MeasurementResult>> = Vec::new();

    // Measure the replay harness using all generated test vectors
    for ktest in ktests {
        // Continue until reaching BKPT 255 (replaystart)
        run_to_replay_start(core).context("Could not continue to the ReplayStart breakpoint")?;
        write_replay_objects(core, &app.variables, &ktest)
            .with_context(|| format!("Could not write to memory with KTest: {:?}", &ktest))?;

        let bkpts = read_breakpoints(core, &ktest, app)?;
        measurements.push(bkpts);
    }

    Ok(measurements)
}

/// Runs to where the replay harness starts. Also runs past any other breakpoints
/// on the way, should there be any.
fn run_to_replay_start(core: &mut Core) -> Result<()> {
    // Wait for core to halt on a breakpoint. If it doesn't something is wrong.
    core.wait_for_core_halted(std::time::Duration::from_secs(DEFAULT_HALT_TIMEOUT_SECONDS))?;
    loop {
        let imm = core::read_breakpoint_value(core)?;
        // Ready to analyze when reaching this breakpoint
        if imm == OtherBreakpoint::ReplayStart as u8 {
            break;
        }
        // Should there be other breakpoints we continue past them
        core::run(core)?;
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
                core.write_8(a, slice).with_context(|| {
                    format!("Could not write {:?} to memory address {:x}", &slice, &a)
                })?;
                core.flush()?;
            }
            None => {
                warn!(
                    "Could not find an address in flash for KTestObject \'{:}\' with the data: {:?}",
                    test.name, test.bytes
                );
            }
        }
    }
    Ok(())
}

/// Read all breakpoints and the cycle counter at their positions from the start of
/// a ReplayStart breakpoint until the next ReplayStart breakpoint. Also writes the
/// generated test vector for a hardware read one at a time in order whenever applicable.
/// Return the measurement result as a list.
///
/// * `core` - A connected probe-rs _core_
/// * `ktest` - The test to replay
/// * `app` - Relevant information of the replay binary
fn read_breakpoints(
    core: &mut Core,
    ktest: &KTest,
    app: &AppInfo,
) -> Result<Vec<MeasurementResult>> {
    let mut measurements: Vec<MeasurementResult> = Vec::new();
    let name = BKPT_UNKNOWN_NAME.to_string();
    let mut current_hw_bkpt: u32 = 0;
    let mut vcell_test_vectors = get_vcell_ktestobjects(ktest);
    vcell_test_vectors.reverse();

    // Loop from breakpoints until the next
    loop {
        core::run(core).context("Could not continue from the ReplayStart breakpoint")?;
        core.wait_for_core_halted(std::time::Duration::from_secs(DEFAULT_HALT_TIMEOUT_SECONDS))
            .context(
                "Core does not halt. Your application might be stuck in a non-terminating loop?",
            )?;

        let current_pc = core::current_pc(core)?;

        // Catch hardware breakpoints which are only used when writing the test vectors
        // for vcell readings to the load register
        if (current_pc == current_hw_bkpt) && (current_hw_bkpt != 0) {
            let reg = get_output_reg_from_breakpoint_addr(app, current_hw_bkpt)?;
            core.clear_hw_breakpoint(current_hw_bkpt)?;
            current_hw_bkpt = 0;

            // It is assumed vcells occur in order so just pop the first test
            if let Some(test) = vcell_test_vectors.pop() {
                write_vcell_test_to_register(core, reg, &test)?;
            }
        // Catch halts that are not breakpoints because that should not happen
        } else if !core::breakpoint_at_pc(core)? {
            return Err(anyhow!(
                "Core halted, but not due to a breakpoint. Can't continue with analysis. Core status: {:?}", core.status()?
            ));
        // Measure breakpoints and
        } else {
            let bkpt_val = core::read_breakpoint_value(core)?;
            let bkpt = Breakpoint::from(bkpt_val);

            match handle_breakpoint(&bkpt, core, &mut measurements, &mut current_hw_bkpt, app)? {
                LoopAction::Break => break,
                LoopAction::Continue => continue,
                LoopAction::Nothing => (),
            }

            // Save the result onto the stack
            let cyccnt = core::read_cycle_counter(core)?;
            measurements.push((bkpt, name.clone(), cyccnt));
        }
    }

    Ok(measurements)
}

/// Tries to get the output/load register from the previous instruction of the current breakpoint
/// address. If a vcell is read then the previous instruction before the breakpoint should be a
/// load register, otherwise it will return an error.
fn get_output_reg_from_breakpoint_addr(app: &AppInfo, breakpoint_address: u32) -> Result<u16> {
    // Fetch the register to overwrite from the previous instruction
    let reg = if app.release {
        let prev_insn_addr = (breakpoint_address - 2) as u64;
        let instruction = app.objdump.get_instruction(&prev_insn_addr).ok_or(anyhow!(
            "Did not find any instruction at address: {:x}",
            &prev_insn_addr
        ))?;
        parse_reg_from_load_instruction(&instruction).ok_or(anyhow!(
            "Could not parse a load register from instruction: {:x?}",
            &instruction
        ))?
    } else {
        0
    };

    Ok(reg)
}

/// Parses the `Rt` register that the load instruction is loading to.
fn parse_reg_from_load_instruction(instruction: &String) -> Option<u16> {
    let mut split = instruction.split(&[' ', ','][..]);
    let mut reg_no: Option<u16> = None;
    if let Some(asm) = split.next() {
        if asm.contains("ld") {
            let reg = split.next().unwrap();
            reg_no = match reg {
                "r0" => Some(0),
                "r1" => Some(1),
                "r2" => Some(2),
                "r3" => Some(3),
                "r4" => Some(4),
                "r5" => Some(5),
                "r6" => Some(6),
                "r7" => Some(7),
                _ => None,
            }
        }
    }
    reg_no
}

/// Writes a test vector for a vcell reading to the given register
fn write_vcell_test_to_register(core: &mut Core, register: u16, test: &KTestObject) -> Result<()> {
    if test.num_bytes == 4 {
        let bytes: [u8; 4] = [test.bytes[0], test.bytes[1], test.bytes[2], test.bytes[3]];
        let data = u32::from_le_bytes(bytes);
        core.write_core_reg(CoreRegisterAddress(register), data)
            .with_context(|| {
                format!(
                    "Could not write data {:?} to register r{}",
                    &data, &register
                )
            })?;
    } else {
        warn!(
            "Failed to overwrite register. Invalid test vector length! Expected 4 bytes, found {:}.",
            test.num_bytes
        );
    }
    Ok(())
}

/// Executes the necessary actions for each valid breakpoint. Measures the cycle count and gets the
/// name for all breakpoints and stores the result. Also sets a HW breakpoint if inside a hardware
/// read.
fn handle_breakpoint(
    bkpt: &Breakpoint,
    core: &mut Core,
    measurements: &mut Vec<MeasurementResult>,
    current_hw_bkpt: &mut u32,
    app: &AppInfo,
) -> Result<LoopAction> {
    let status = match bkpt {
        // On ReplayStart the loop is complete
        Breakpoint::Other(OtherBreakpoint::ReplayStart) => LoopAction::Break,
        // Save the name and continue to the next loop iteration
        Breakpoint::Other(OtherBreakpoint::InsideTask) => {
            let name = read_breakpoint_task_name(core, &app.subprograms)?;
            let (b, _, u) = measurements.pop().unwrap();
            measurements.push((b, name, u));

            LoopAction::Continue
        }
        // Save the name and continue to the next loop iteration
        Breakpoint::Other(OtherBreakpoint::InsideLock) => {
            let name = read_breakpoint_lock_name(core, &app.resource_locks)?;
            let (b, _, u) = measurements.pop().unwrap();
            measurements.push((b, name, u));

            LoopAction::Continue
        }
        // If inside a hardware read, set hardware breakpoint before exiting the reading
        Breakpoint::Other(OtherBreakpoint::InsideHardwareRead) => {
            // Get all vcells in range of this lock and update vcell_stack
            if let Some(mut current_vcell) = get_current_vcell_from_lr(core, &app.vcells)? {
                if current_vcell.ranges.is_empty() {
                    return Err(anyhow!("Subroutine has no address ranges"));
                }
                let (_, high_pc) = current_vcell.ranges.pop().unwrap();
                *current_hw_bkpt = high_pc as u32;
                core.set_hw_breakpoint(*current_hw_bkpt)?;
            }

            LoopAction::Continue
        }
        // Ignore everything else for now
        _ => LoopAction::Nothing,
    };
    Ok(status)
}

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
