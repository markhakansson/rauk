use crate::cli::Flashing;

use crate::metadata::RaukInfo;
use crate::utils::core as core_utils;
use anyhow::{Context, Result};
use probe_rs::flashing::{download_file, Format};
use std::path::PathBuf;
use std::process::{Command, ExitStatus};

const HALT_TIMEOUT_SECONDS: u64 = 5;

/// Builds the replay harness and flashes it to the target hardware.
/// Returns the path to the built executable.
pub fn flash_to_target(opts: &Flashing, metadata: &RaukInfo) -> Result<PathBuf> {
    let mut target_dir = metadata.project_directory.clone();
    let mut cargo_path = metadata.project_directory.clone();
    target_dir.push("target/");
    cargo_path.push("Cargo.toml");

    build_replay_harness(&opts, &mut cargo_path, &mut target_dir)
        .context("Failed to build the replay harness")?;

    let mut session = core_utils::open_and_attach_probe(&opts.chip)?;

    // Flash the card with binary
    download_file(&mut session, &target_dir.as_path(), Format::Elf)?;

    // Reset the core and halt
    let mut core = session.core(0)?;
    core.reset_and_halt(std::time::Duration::from_secs(HALT_TIMEOUT_SECONDS))?;

    Ok(target_dir)
}

/// Builds the replay harness by setting the correct features for all patched
/// crates.
fn build_replay_harness(
    a: &Flashing,
    cargo_path: &mut PathBuf,
    target_dir: &mut PathBuf,
) -> Result<ExitStatus, std::io::Error> {
    let mut cargo = Command::new("cargo");
    cargo.arg("build");

    if a.target.is_some() {
        let target = a.target.clone().unwrap();
        cargo.args(&["--target", target.as_str()]);
        target_dir.push(target);
    }

    if a.release {
        cargo.arg("--release");
        target_dir.push("release/");
    } else {
        target_dir.push("debug/");
    }

    let name: String;
    if a.example.is_none() {
        name = a.bin.as_ref().unwrap().to_string();
        cargo.args(&["--bin", name.as_str()]);
    } else {
        name = a.example.as_ref().unwrap().to_string();
        cargo.args(&["--example", name.as_str()]);
    }
    target_dir.push(name);

    cargo
        .args(&["--features", "klee-replay"])
        .args(&["--manifest-path", cargo_path.to_str().unwrap()]);

    cargo.status()
}
