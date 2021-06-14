mod breakpoints;
mod dwarf;
mod hardware;
mod objdump;
mod trace;

use crate::cli::MeasureInput;
use crate::metadata::RaukInfo;
use crate::utils::{core as core_utils, klee};
use crate::RaukSettings;
use anyhow::{anyhow, Context, Result};
use hardware::MeasurementResult;
use object::Object;
use std::path::PathBuf;
use std::{borrow, fs};
use trace::Trace;

const RAUK_JSON_OUTPUT: &str = "rauk.json";

pub fn wcet_measurement(
    input: &MeasureInput,
    settings: &RaukSettings,
    metadata: &RaukInfo,
) -> Result<Option<PathBuf>> {
    let (dwarf_path, ktests_path) = get_analysis_paths(&input, &metadata)?;
    let mut updated_input = input.clone();
    updated_input.get_missing_input(settings);

    let objdump = objdump::disassemble(&dwarf_path).context("Could not disassemble the binary")?;

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

    println!("parsing ktests");
    let ktests = klee::parse_ktest_files(&ktests_path)?;

    if ktests.is_empty() {
        return Err(anyhow!(
            "No test vectors found. Cannot continue with WCET measurement without test vectors"
        ));
    }

    println!("getting replay addresses");
    let addr = dwarf::get_replay_addresses(&dwarf)?;
    println!("getting subprograms");
    let subprograms = dwarf::get_subprograms(&dwarf)?;
    println!("getting subroutines");
    let subroutines = dwarf::get_subroutines(&dwarf)?;
    println!("getting resources");
    let resources = dwarf::get_resources_from_subroutines(&subroutines);

    let mut vcells = dwarf::get_vcell_from_subroutines(&subroutines);
    println!("vcells:");
    println!("{:x?}", &vcells);
    let mut session = if let Some(chip) = updated_input.chip {
        core_utils::open_and_attach_probe(&chip)?
    } else {
        return Err(anyhow!(
            "Cannot attach to hardware. No chip type given as input"
        ));
    };
    let mut core = session.core(0)?;

    let measurements = hardware::measure_replay_harness(
        &mut core,
        &ktests,
        &addr,
        &subprograms,
        &resources,
        &mut vcells,
        &objdump,
        input.release,
    )
    .context("Could not complete the measurement of the replay harness")?;

    let traces = post_measurement_analysis(measurements)
        .context("Could not complete the analysis of measurement data")?;
    println!("{:#?}", traces);

    let output_path = save_traces_to_directory(&traces, &metadata.project_directory)?;

    Ok(Some(output_path))
}

fn post_measurement_analysis(measurements: Vec<Vec<MeasurementResult>>) -> Result<Vec<Trace>> {
    let mut traces: Vec<Trace> = Vec::new();
    for measurement in measurements {
        if let Ok(mut trace) = trace::wcet_analysis(measurement) {
            traces.append(&mut trace);
        }
    }
    Ok(traces)
}

/// Get the necessary paths for analysis.
fn get_analysis_paths(input: &MeasureInput, metadata: &RaukInfo) -> Result<(PathBuf, PathBuf)> {
    let dwarf_path: PathBuf = match &input.dwarf {
        Some(path) => path.clone(),
        None => match metadata.get_dwarf_path() {
            Some(path) => path,
            None => return Err(anyhow!("No path to DWARF was given/found")),
        },
    };

    let ktests_path: PathBuf = match &input.ktests {
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
