mod analysis;
mod breakpoints;
mod dwarf;
mod measurement;

use crate::cli::Analysis;
use crate::metadata::RaukInfo;
use crate::utils::{klee, probe as core_utils};
use analysis::Trace;
use anyhow::{anyhow, Context, Result};
use measurement::MeasurementResult;
use object::Object;
use std::path::PathBuf;
use std::{borrow, fs};

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
    let mut measurements: Vec<Vec<MeasurementResult>> = Vec::new();

    // Measurement on hardware
    for ktest in ktests {
        // Continue until reaching BKPT 255 (replaystart)
        measurement::run_to_replay_start(&mut core)
            .context("Could not continue to replay start")?;
        measurement::write_replay_objects(&mut core, &ktest, &addr)
            .with_context(|| format!("Could not write to memory with KTest: {:?}", &ktest))?;
        let bkpts = measurement::read_breakpoints(&mut core, &subprograms, &resources)?;
        measurements.push(bkpts);
    }

    // Post-measurement analysis
    for measurement in measurements {
        if let Ok(mut trace) = analysis::wcet_analysis(measurement) {
            traces.append(&mut trace);
        }
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
