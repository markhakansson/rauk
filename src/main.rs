mod cargo;
mod cli;
mod flash;
mod generate;
mod measure;
mod metadata;
mod settings;
mod utils;

use anyhow::{anyhow, Context, Result};
use cli::{BuildDetails, CliOptions, Command};
use metadata::{ArtifactDetail, OutputInfo, RaukMetadata};
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
        complete_rauk_cleanup(&project_dir)
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
            post_execution_cleanup(&project_dir_copy, no_patch).unwrap();
        })?;

        // Load metadata and check if previous execution was ok
        let mut metadata = RaukMetadata::new(&project_dir);
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
        post_execution_cleanup(&project_dir, opts.no_patch)?;
        metadata.previous_execution.gracefully_terminated = true;
        metadata.save()?;

        res
    }
}

fn match_cli_opts(
    opts: &CliOptions,
    settings: &RaukSettings,
    metadata: &mut RaukMetadata,
) -> Result<()> {
    match &opts.cmd {
        Command::Generate(g) => {
            let path = generate::generate_klee_tests(g, &metadata)
                .context("Failed to execute generate command")?;
            update_metadata_output(&g.build, metadata, Some(path), &opts.cmd)?;
        }
        Command::Flash(f) => {
            let path = flash::flash_to_target(f, &settings, &metadata)
                .context("Failed to execute flash command")?;
            update_metadata_output(&f.build, metadata, Some(path), &opts.cmd)?;
        }
        Command::Measure(a) => {
            let path = measure::wcet_measurement(a, &settings, &metadata)
                .context("Failed to execute analyze command")?;
            update_metadata_output(&a.build, metadata, path, &opts.cmd)?;
        }
        _ => (),
    }

    Ok(())
}

fn update_metadata_output(
    build: &BuildDetails,
    metadata: &mut RaukMetadata,
    path: Option<PathBuf>,
    command: &Command,
) -> Result<()> {
    let name = build.get_name();
    let example = build.is_example();
    let release = build.is_release();

    let output = OutputInfo::new(path.clone());

    let opt = metadata.get_mut_artifact_detail(&name, release, example);
    let mut artifact = if let Some(artifact) = opt {
        artifact.clone()
    } else {
        ArtifactDetail::new()
    };

    match command {
        Command::Generate(_) => artifact.generate_output = Some(output),
        Command::Flash(_) => artifact.flash_output = Some(output),
        Command::Measure(_) => artifact.measure_output = Some(output),
        _ => return Err(anyhow!("Cannot store metadata for command: {:?}", &command)),
    }

    metadata.insert(&name, artifact, release, example);

    Ok(())
}

/// Cleanup before exiting the program
fn post_execution_cleanup(project_dir: &PathBuf, no_patch: bool) -> Result<()> {
    // Restore original Cargo.toml
    if no_patch {
        Ok(())
    } else {
        cargo::restore_orignal_cargo_toml(&project_dir)
    }
}

/// Manual cleanup procedure. Removes metadata only.
fn complete_rauk_cleanup(project_dir: &PathBuf) -> Result<()> {
    let mut metadata_path = project_dir.clone();
    let mut rauk_cargo_toml = project_dir.clone();
    metadata_path.push(metadata::RAUK_METADATA_OUTPUT);
    rauk_cargo_toml.push(cargo::RAUK_CARGO_TOML);
    remove_file(&metadata_path)?;
    remove_file(&rauk_cargo_toml)?;
    Ok(())
}
