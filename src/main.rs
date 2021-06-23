mod cargo;
mod cli;
mod flash;
mod generate;
mod logger;
mod measure;
mod metadata;
mod settings;
mod utils;

#[macro_use]
extern crate log;
use anyhow::{Context, Result};
use cli::{CliOptions, Command};
use metadata::RaukMetadata;
use settings::RaukSettings;
use std::fs::{canonicalize, create_dir_all, remove_dir_all, remove_file};
use std::os::unix::fs::symlink;
use std::path::PathBuf;

fn main() -> Result<()> {
    let mut opts = cli::get_cli_opts();
    let project_dir = match opts.path.clone() {
        Some(path) => canonicalize(path)?,
        None => canonicalize(PathBuf::from("./"))?,
    };

    logger::init_logger(&project_dir, opts.verbose)?;

    if opts.cmd == Command::Cleanup {
        complete_rauk_cleanup(&project_dir)
    } else {
        // Handle SIGINT and SIGTERM
        let no_patch = opts.no_patch;
        let project_dir_copy = project_dir.clone();
        ctrlc::set_handler(move || {
            post_execution_cleanup(&project_dir_copy, no_patch).unwrap();
        })?;

        let _ = create_dir_all(&project_dir.join(metadata::RAUK_OUTPUT_DIR));

        let settings = settings::load_settings(&project_dir)?;
        let mut metadata = metadata::load_metadata(&project_dir)?;

        // Patch the project's Cargo.toml
        if !opts.no_patch {
            cargo::backup_original_cargo_files(&project_dir)?;
            info!("User Cargo.toml backed up");
            cargo::update_custom_cargo_toml(&project_dir)?;
            cargo::change_cargo_toml_to_custom(&project_dir)?;
            info!("Custom Cargo.toml patched");
        }

        // Save the result, need to do some cleanup before returning it
        let res = match_cli_opts(&mut opts, &settings, &mut metadata);

        // Cleanup and save metadata
        post_execution_cleanup(&project_dir, opts.no_patch)?;
        metadata.program_execution_successful();
        metadata.save()?;

        res
    }
}

fn match_cli_opts(
    opts: &mut CliOptions,
    settings: &RaukSettings,
    metadata: &mut RaukMetadata,
) -> Result<()> {
    // Inherit verbose flag from main cli opts
    match &mut opts.cmd {
        Command::Generate(g) => g.verbose = opts.verbose,
        Command::Flash(f) => f.verbose = opts.verbose,
        _ => (),
    }

    match &opts.cmd {
        Command::Generate(g) => {
            let path = generate::generate_klee_tests(g, &metadata)
                .context("Failed to execute generate command")?;
            let _ = symlink(&path, &metadata.rauk_output_directory.join("klee-last"));
            metadata.update_output(&g.build, Some(path), &opts.cmd)?;
        }
        Command::Flash(f) => {
            let path = flash::flash_to_target(f, &settings, &metadata)
                .context("Failed to execute flash command")?;
            metadata.update_output(&f.build, Some(path), &opts.cmd)?;
        }
        Command::Measure(a) => {
            let path = measure::wcet_measurement(a, &settings, &metadata)
                .context("Failed to execute analyze command")?;
            metadata.update_output(&a.build, path, &opts.cmd)?;
        }
        _ => (),
    }

    Ok(())
}

/// Cleanup before exiting the program
fn post_execution_cleanup(project_dir: &PathBuf, no_patch: bool) -> Result<()> {
    // Restore original Cargo.toml
    if !no_patch {
        cargo::restore_orignal_cargo_files(&project_dir)?;
        info!("User Cargo.toml restored");
    }

    Ok(())
}

/// Manual cleanup procedure. Removes metadata only.
fn complete_rauk_cleanup(project_dir: &PathBuf) -> Result<()> {
    let rauk_cargo_toml = project_dir.join(cargo::RAUK_CARGO_TOML);
    let rauk_output_path = metadata::get_rauk_output_path(&project_dir);
    let _ = remove_dir_all(&rauk_output_path);
    let _ = remove_file(&rauk_cargo_toml);
    info!("Completed cleanup procedure of rauk data");
    Ok(())
}
