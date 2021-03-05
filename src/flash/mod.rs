use crate::cli::Flashing;

use anyhow::Result;
use probe_rs::{
    flashing::{download_file, Format},
    Probe,
};
use std::path::PathBuf;
use std::process::{Command, ExitStatus};

/// Builds the replay harness and flashes it to the target hardware.
/// Returns the path to the built executable.
pub fn flash_to_target(opts: Flashing) -> Result<PathBuf> {
    let project_dir = match opts.path.clone() {
        Some(path) => path,
        None => PathBuf::from("./"),
    };
    let mut target_dir = project_dir.clone();
    let mut cargo_path = project_dir.clone();
    target_dir.push("target/");
    cargo_path.push("Cargo.toml");

    build_replay_harness(&opts, &mut cargo_path, &mut target_dir)?;

    // Get a list of all available debug probes.
    let probes = Probe::list_all();

    // Use the first probe found.
    let probe = probes[0].open()?;

    // Attach to a chip.
    let mut session = probe.attach(opts.chip)?;

    // Flash the card with binary
    download_file(&mut session, &target_dir.as_path(), Format::Elf)?;

    // Reset the core and halt
    {
        let mut core = session.core(0).unwrap();
        core.reset_and_halt(std::time::Duration::from_secs(1))?;
    }

    Ok(target_dir)
}

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
