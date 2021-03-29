mod analysis;
mod dwarf;

use self::dwarf::{Subprogram, Subroutine};
use crate::cli::Analysis;
use crate::utils::{klee::parse_ktest_files, probe as core_utils};
use analysis::{Breakpoint, Other};
use anyhow::{anyhow, Context, Result};
use dwarf::ObjectLocationMap;
use gimli::{read::Dwarf, EndianSlice, RunTimeEndian};
use ktest_parser::KTest;
use object::Object;
use probe_rs::{Core, MemoryInterface, Probe};
use std::{borrow, fs};

const HALT_TIMEOUT_SECONDS: u64 = 5;

pub fn analyze(a: Analysis) -> Result<()> {
    let file = fs::File::open(&a.dwarf)?;
    let mmap = unsafe { memmap::Mmap::map(&file)? };
    let object = object::File::parse(&*mmap)?;
    let endian = if object.is_little_endian() {
        gimli::RunTimeEndian::Little
    } else {
        gimli::RunTimeEndian::Big
    };
    let dwarf_cow = dwarf::load_dwarf_from_file(object)?;

    // Borrow a `Cow<[u8]>` to create an `EndianSlice`.
    let borrow_section: &dyn for<'a> Fn(
        &'a borrow::Cow<[u8]>,
    ) -> gimli::EndianSlice<'a, gimli::RunTimeEndian> =
        &|section| gimli::EndianSlice::new(&*section, endian);

    // Create `EndianSlice`s for all of the sections.
    let dwarf = dwarf_cow.borrow(&borrow_section);

    let ktests = parse_ktest_files(&a.ktests);
    let addr = dwarf::get_replay_addresses(&dwarf)?;
    let subprograms = dwarf::get_subprograms(&dwarf)?;
    let subroutines = dwarf::get_subroutines(&dwarf)?;
    println!("{:#x?}", ktests);
    println!("{:#x?}", addr);

    let probes = Probe::list_all();
    let probe = probes[0].open()?;

    let mut session = probe.attach(a.chip)?;

    let mut core = session.core(0)?;

    // Analysis
    for ktest in ktests {
        println!("-------------------------------------------------------------");
        // Continue until reaching BKPT 255 (replaystart)
        run_to_replay_start(&mut core).context("Could not continue to replay start")?;
        write_replay_objects(&mut core, &ktest, &addr)
            .with_context(|| format!("Could not replay with KTest: {:?}", &ktest))?;
        let bkpts = read_breakpoints(&mut core, &subprograms, &subroutines)?;
        println!("{:#?}", bkpts);
        let trace = analysis::wcet_analysis(bkpts);
        println!("{:#?}", trace);
    }

    Ok(())
}

pub fn test_dwarf(a: Analysis) -> Result<()> {
    let file = fs::File::open(&a.dwarf)?;
    let mmap = unsafe { memmap::Mmap::map(&file)? };
    let object = object::File::parse(&*mmap)?;
    let endian = if object.is_little_endian() {
        gimli::RunTimeEndian::Little
    } else {
        gimli::RunTimeEndian::Big
    };
    let dwarf_cow = dwarf::load_dwarf_from_file(object)?;

    // Borrow a `Cow<[u8]>` to create an `EndianSlice`.
    let borrow_section: &dyn for<'a> Fn(
        &'a borrow::Cow<[u8]>,
    ) -> gimli::EndianSlice<'a, gimli::RunTimeEndian> =
        &|section| gimli::EndianSlice::new(&*section, endian);

    // Create `EndianSlice`s for all of the sections.
    let dwarf = dwarf_cow.borrow(&borrow_section);

    let subprograms = dwarf::get_subprograms(&dwarf)?;
    //println!("{:#?}", &subprograms);
    //let addr = 0x800020d;
    //let in_range = dwarf::get_subprograms_in_range(&subprograms, addr)?;

    //println!("{:#?}", &in_range);
    // println!(
    //     "Shortest range: {:#?}",
    //     dwarf::get_shortest_range_subprogram(&in_range)?
    // );

    println!("----- INLINED SUBROUTINES -----");
    let subroutines = dwarf::get_subroutines(&dwarf)?;
    println!("{:#?}", &subroutines);
    let tst_addr = 0x0800_0277;
    let subroutines_in_range = dwarf::get_subroutines_in_range(&subroutines, tst_addr);
    println!("----- IN RANGE INLINED SUBROUTINES -----");
    println!("{:#?}", subroutines_in_range);
    Ok(())
}

/// Runs to where the replay starts.
fn run_to_replay_start(core: &mut Core) -> Result<()> {
    // Wait for core to halt on a breakpoint. If it doesn't
    // something is wrong.
    core.wait_for_core_halted(std::time::Duration::from_secs(HALT_TIMEOUT_SECONDS))?;
    loop {
        let imm = core_utils::read_breakpoint_value(core)?;
        // Ready to analyze when reaching this breakpoint
        if imm == Other::ReplayStart as u8 {
            break;
        }
        // Should there be other breakpoints we continue past them
        core_utils::run(core)?;
    }
    Ok(())
}

/// Writes the replay contents of the KTEST file to the objects memory addresses.
fn write_replay_objects(
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
                return Err(anyhow!("Address was not found"));
            }
        }
    }
    Ok(())
}

/// Read all breakpoints and the cycle counter at their positions
/// between the ReplayStart breakpoints and return them as a list
fn read_breakpoints(
    core: &mut Core,
    subprograms: &Vec<Subprogram>,
    subroutines: &Vec<Subroutine>,
) -> Result<Vec<(Breakpoint, String, u32)>> {
    let mut stack: Vec<(Breakpoint, String, u32)> = Vec::new();
    let mut name = "".to_string();

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
            Breakpoint::Other(Other::ReplayStart) => break,
            // Save the name and continue to the next loop iteration
            Breakpoint::Other(Other::InsideTask) => {
                let name = read_breakpoint_task_name(core, &subprograms)?;
                let (b, _, u) = stack.pop().unwrap();
                stack.push((b, name, u));

                continue;
            }
            Breakpoint::Other(Other::InsideLock) => {
                name = read_breakpoint_lock_name(core, &subroutines)?;
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

/// Tries to read the name of the current scope of the breakpoint from DWARF
fn read_breakpoint_task_name(core: &mut Core, subprograms: &Vec<Subprogram>) -> Result<String> {
    let lr = core.registers().return_address();
    let lr_val = core.read_core_reg(lr)?;

    let in_range = dwarf::get_subprograms_in_range(subprograms, lr_val as u64)?;
    let optimal = dwarf::get_shortest_range_subprogram(&in_range)?;

    println!("LR VALUE IS: {:x?}", lr_val);
    let name = match optimal {
        Some(s) => s.name,
        None => "".to_string(),
    };
    Ok(name)
}

fn read_breakpoint_lock_name(core: &mut Core, subroutines: &Vec<Subroutine>) -> Result<String> {
    let lr = core.registers().return_address();
    let lr_val = core.read_core_reg(lr)?;
    println!("LR VALUE IS: {:x?}", lr_val);
    let string = "".to_string();
    Ok(string)
}
