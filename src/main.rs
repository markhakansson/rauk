mod analysis;
mod cli;
mod config;
mod flash;
mod generate;
mod metadata;
mod utils;

use anyhow::{Context, Result};
use cli::{CliOptions, Command};
use metadata::{OutputInfo, RaukInfo};
use std::fs::canonicalize;
use std::path::PathBuf;
use utils::cargo;

fn main() -> Result<()> {
    let opts = cli::get_cli_opts();
    let project_dir = match opts.path.clone() {
        Some(path) => canonicalize(path)?,
        None => canonicalize(PathBuf::from("./"))?,
    };

    // Handle SIGINT and SIGTERM
    let no_patch = opts.no_patch;
    let project_dir_copy = project_dir.clone();
    ctrlc::set_handler(move || {
        post_cleanup(&project_dir_copy, no_patch).unwrap();
    })?;

    // Patch the project's Cargo.toml
    if !opts.no_patch {
        cargo::backup_original_cargo_toml(&project_dir)?;
        cargo::update_custom_cargo_toml(&project_dir)?;
        cargo::change_cargo_toml_to_custom(&project_dir)?;
    }

    let mut metadata = RaukInfo::new(&project_dir);
    metadata.load()?;

    // Save the result, need to do some cleanup before returning it
    let res = match_cli_opts(&opts, &mut metadata);

    metadata.save()?;
    post_cleanup(&project_dir, opts.no_patch)?;

    res
}

fn match_cli_opts(opts: &CliOptions, metadata: &mut RaukInfo) -> Result<()> {
    match &opts.cmd {
        Command::Generate(g) => {
            let path = generate::generate_klee_tests(g, &metadata)
                .context("Failed to execute generate command")?;
            //println!("{:#?}", path);
            let info = OutputInfo::new(Some(path.clone()));
            metadata.generate_output = Some(info);
        }
        Command::Analyze(a) => {
            let path =
                analysis::analyze(a, &metadata).context("Failed to execute analyze command")?;
            let info = OutputInfo::new(path.clone());
            metadata.analyze_output = Some(info);
        }
        Command::Flash(f) => {
            let path =
                flash::flash_to_target(f, &metadata).context("Failed to execute flash command")?;
            let info = OutputInfo::new(Some(path.clone()));
            metadata.flash_output = Some(info);
        }
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
