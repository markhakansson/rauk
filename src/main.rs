mod analysis;
mod cargo;
mod cli;
mod flash;
mod generate;
mod measure;
mod metadata;
mod settings;
mod utils;

use anyhow::{Context, Result};
use cli::{CliOptions, Command};
use metadata::{OutputInfo, RaukInfo};
use settings::RaukSettings;
use std::fs::{canonicalize, remove_file};
use std::path::PathBuf;

fn main() -> Result<()> {
    let opts = cli::get_cli_opts();
    let project_dir = match opts.path.clone() {
        Some(path) => canonicalize(path)?,
        None => canonicalize(PathBuf::from("./"))?,
    };

    if opts.cmd == Command::Cleanup {
        cleanup(&project_dir)
    } else {
        let settings: RaukSettings = if settings::settings_file_exists(&project_dir) {
            settings::load_settings_from_dir(&project_dir)?
        } else {
            RaukSettings::new()
        };

        // Handle SIGINT and SIGTERM
        let no_patch = opts.no_patch;
        let project_dir_copy = project_dir.clone();
        ctrlc::set_handler(move || {
            post_cleanup(&project_dir_copy, no_patch).unwrap();
        })?;

        // Load metadata and check if previous execution was ok
        let mut metadata = RaukInfo::new(&project_dir);
        metadata.load()?;

        // Patch the project's Cargo.toml
        if !opts.no_patch {
            cargo::backup_original_cargo_toml(&project_dir)?;
            cargo::update_custom_cargo_toml(&project_dir)?;
            cargo::change_cargo_toml_to_custom(&project_dir)?;
        }

        // Save the result, need to do some cleanup before returning it
        let res = match_cli_opts(&opts, &settings, &mut metadata);

        // Cleanup and save metadata
        post_cleanup(&project_dir, opts.no_patch)?;
        metadata.previous_execution.gracefully_terminated = true;
        metadata.save()?;

        res
    }
}

fn match_cli_opts(
    opts: &CliOptions,
    settings: &RaukSettings,
    metadata: &mut RaukInfo,
) -> Result<()> {
    match &opts.cmd {
        Command::Generate(g) => {
            let path = generate::generate_klee_tests(g, &metadata)
                .context("Failed to execute generate command")?;
            //println!("{:#?}", path);
            let info = OutputInfo::new(Some(path.clone()));
            metadata.generate_output = Some(info);
        }
        Command::Flash(f) => {
            let path = flash::flash_to_target(f, &settings, &metadata)
                .context("Failed to execute flash command")?;
            let info = OutputInfo::new(Some(path.clone()));
            metadata.flash_output = Some(info);
        }
        Command::Measure(a) => {
            let path = measure::wcet_measurement(a, &settings, &metadata)
                .context("Failed to execute analyze command")?;
            let info = OutputInfo::new(path.clone());
            metadata.analyze_output = Some(info);
        }
        Command::Analyze(input) => {
            analysis::response_times(input)?;
        }
        Command::Cleanup => (),
    }

    Ok(())
}

/// Cleanup before exiting the program
fn post_cleanup(project_dir: &PathBuf, no_patch: bool) -> Result<()> {
    // Restore original Cargo.toml
    if no_patch {
        Ok(())
    } else {
        cargo::restore_orignal_cargo_toml(&project_dir)
    }
}

/// Manual cleanup procedure. Removes metadata only.
fn cleanup(project_dir: &PathBuf) -> Result<()> {
    let mut metadata_path = project_dir.clone();
    let mut rauk_cargo_toml = project_dir.clone();
    metadata_path.push(metadata::RAUK_OUTPUT_INFO);
    rauk_cargo_toml.push(cargo::RAUK_CARGO_TOML);
    remove_file(&metadata_path)?;
    remove_file(&rauk_cargo_toml)?;
    Ok(())
}
