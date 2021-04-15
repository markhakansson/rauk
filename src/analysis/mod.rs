mod analysis;
mod dwarf;
mod measurement;

use crate::cli::Analysis;
use crate::utils::{klee, probe as core_utils};
use analysis::Trace;
use anyhow::{Context, Result};
use dwarf::ObjectLocationMap;
use ktest_parser::KTest;
use object::Object;
use probe_rs::{Core, MemoryInterface};
use std::{borrow, fs};

const HALT_TIMEOUT_SECONDS: u64 = 5;

pub fn analyze(a: &Analysis) -> Result<()> {
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

    let ktests = klee::parse_ktest_files(&a.ktests)?;
    let addr = dwarf::get_replay_addresses(&dwarf)?;
    let subprograms = dwarf::get_subprograms(&dwarf)?;
    let subroutines = dwarf::get_subroutines(&dwarf)?;

    let mut session = core_utils::open_and_attach_probe(&a.chip)?;
    let mut core = session.core(0)?;

    let mut traces: Vec<Trace> = Vec::new();

    // Analysis
    for ktest in ktests {
        // Continue until reaching BKPT 255 (replaystart)
        run_to_replay_start(&mut core).context("Could not continue to replay start")?;
        write_replay_objects(&mut core, &ktest, &addr)
            .with_context(|| format!("Could not write to memory with KTest: {:?}", &ktest))?;
        let bkpts = measurement::read_breakpoints(&mut core, &subprograms, &subroutines)?;
        let mut trace = match analysis::wcet_analysis(bkpts) {
            Ok(trace) => trace,
            Err(_) => continue,
        };
        traces.append(&mut trace);
    }

    match &a.output {
        Some(dir) => {
            let mut path = dir.clone();
            path.push("rauk.json");
            let serialized = serde_json::to_string(&traces)?;
            fs::write(path, serialized)?;
        }
        None => {
            println!("{:#?}", traces);
        }
    }

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
        if imm == measurement::OtherBreakpoint::ReplayStart as u8 {
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
