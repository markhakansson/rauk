use crate::cli::FlashInput;
use crate::metadata::RaukMetadata;
use crate::settings::RaukSettings;
use crate::utils::core as core_utils;
use anyhow::{anyhow, Context, Result};
use probe_rs::flashing::{download_file, Format};
use std::path::PathBuf;
use std::process::{Command, ExitStatus};

const DEFAULT_HALT_TIMEOUT_SECONDS: u64 = 5;

/// Builds the replay harness and flashes it to the target hardware.
/// Returns the path to the built executable.
pub fn flash_to_target(
    input: &FlashInput,
    settings: &RaukSettings,
    metadata: &RaukMetadata,
) -> Result<PathBuf> {
    let mut target_dir = metadata.project_directory.clone();
    let mut cargo_path = metadata.project_directory.clone();
    target_dir.push("target/");
    cargo_path.push("Cargo.toml");

    let mut updated_input = input.clone();
    updated_input.get_missing_input(settings);
    let halt_timeout = updated_input
        .halt_timeout
        .unwrap_or(DEFAULT_HALT_TIMEOUT_SECONDS);

    build_replay_harness(&updated_input, &mut cargo_path, &mut target_dir)
        .context("Failed to build the replay harness")?;
    let mut session = if let Some(chip) = updated_input.chip {
        core_utils::open_and_attach_probe(&chip)?
    } else {
        return Err(anyhow!(
            "Can't attach to hardware. No chip type given as input"
        ));
    };

    // Flash the card with binary
    download_file(&mut session, &target_dir.as_path(), Format::Elf)
        .context("Could not flash replay harness to hardware")?;

    // Reset the core and halt
    let mut core = session.core(0)?;
    core.reset_and_halt(std::time::Duration::from_secs(halt_timeout))?;

    Ok(target_dir)
}

/// Builds the replay harness by setting the correct features for all patched
/// crates.
fn build_replay_harness(
    input: &FlashInput,
    cargo_path: &mut PathBuf,
    target_dir: &mut PathBuf,
) -> Result<ExitStatus, std::io::Error> {
    let mut cargo = Command::new("cargo");
    cargo.arg("rustc");

    if input.target.is_some() {
        let target = input.target.clone().unwrap();
        cargo.args(&["--target", target.as_str()]);
        target_dir.push(target);
    }

    if input.is_release() {
        cargo.arg("--release");
        target_dir.push("release/");
    } else {
        target_dir.push("debug/");
    }

    if input.verbose {
        cargo.arg("--verbose");
    }

    let name: String;
    if input.build.example.is_none() {
        name = input.build.bin.as_ref().unwrap().to_string();
        cargo.args(&["--bin", name.as_str()]);
    } else {
        name = input.build.example.as_ref().unwrap().to_string();
        cargo.args(&["--example", name.as_str()]);
        target_dir.push("examples/");
    }
    target_dir.push(name);

    cargo
        .args(&["--features", "klee-replay"])
        .args(&["--manifest-path", cargo_path.to_str().unwrap()])
        .arg("--")
        .args(&["-C", "linker-plugin-lto"]);

    cargo.status()
}
