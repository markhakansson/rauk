mod analysis;
mod dwarf;
mod measurement;

use crate::cli::Analysis;
use crate::metadata::RaukInfo;
use crate::utils::{klee, probe as core_utils};
use analysis::Trace;
use anyhow::{anyhow, Context, Result};
use dwarf::ObjectLocationMap;
use ktest_parser::KTest;
use object::Object;
use probe_rs::{Core, MemoryInterface};
use std::path::PathBuf;
use std::{borrow, fs};

const HALT_TIMEOUT_SECONDS: u64 = 5;
const RAUK_JSON_OUTPUT: &str = "rauk.json";

pub fn analyze(a: &Analysis, metadata: &RaukInfo) -> Result<Option<PathBuf>> {
    let (dwarf_path, ktests_path) = get_analysis_paths(&a, &metadata)?;

    let file = fs::File::open(dwarf_path)?;
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

    let ktests = klee::parse_ktest_files(&ktests_path)?;
    let addr = dwarf::get_replay_addresses(&dwarf)?;
    let subprograms = dwarf::get_subprograms(&dwarf)?;
    let subroutines = dwarf::get_subroutines(&dwarf)?;
    let resources = dwarf::get_resources_from_subroutines(&subroutines);
    let vcells = dwarf::get_vcell_from_subroutines(&subroutines);

    println!("subprograms:\n {:#x?}", subprograms);
    println!("vcells:\n {:#x?}", vcells);

    let mut session = core_utils::open_and_attach_probe(&a.chip)?;
    let mut core = session.core(0)?;

    let mut traces: Vec<Trace> = Vec::new();

    // Analysis
    for ktest in ktests {
        // Continue until reaching BKPT 255 (replaystart)
        run_to_replay_start(&mut core).context("Could not continue to replay start")?;
        write_replay_objects(&mut core, &ktest, &addr)
            .with_context(|| format!("Could not write to memory with KTest: {:?}", &ktest))?;
        let bkpts = measurement::read_breakpoints(&mut core, &subprograms, &resources)?;
        let mut trace = match analysis::wcet_analysis(bkpts) {
            Ok(trace) => trace,
            Err(_) => continue,
        };
        traces.append(&mut trace);
    }

    println!("{:#?}", traces);

    let output_path = save_traces_to_directory(&traces, &metadata.project_directory)?;
    Ok(Some(output_path))
}

/// Get the necessary paths for analysis.
fn get_analysis_paths(a: &Analysis, metadata: &RaukInfo) -> Result<(PathBuf, PathBuf)> {
    let dwarf_path: PathBuf = match &a.dwarf {
        Some(path) => path.clone(),
        None => match metadata.get_dwarf_path() {
            Some(path) => path,
            None => return Err(anyhow!("No path to DWARF was given/found")),
        },
    };

    let ktests_path: PathBuf = match &a.ktests {
        Some(path) => path.clone(),
        None => match metadata.get_ktest_path() {
            Some(path) => path,
            None => return Err(anyhow!("No path to KTESTS found/given")),
        },
    };

    Ok((dwarf_path, ktests_path))
}

/// Saves the analysis result to project directory.
fn save_traces_to_directory(traces: &Vec<Trace>, project_dir: &PathBuf) -> Result<PathBuf> {
    let mut path = project_dir.clone();
    path.push(RAUK_JSON_OUTPUT);
    let serialized = serde_json::to_string(traces)?;
    fs::write(&path, serialized)?;
    Ok(path)
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
