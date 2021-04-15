mod analysis;
mod cli;
mod config;
mod flash;
mod generate;
mod utils;

use anyhow::{Context, Result};
use cli::{CliOptions, Command};
use std::path::PathBuf;
use utils::cargo;

fn main() -> Result<()> {
    let opts = cli::get_cli_opts();
    let project_dir = match opts.path.clone() {
        Some(path) => path,
        None => PathBuf::from("./"),
    };

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

    // Save the result, need to do some cleanup before returning it
    let res = match_cli_opts(&opts, &project_dir);

    post_cleanup(&project_dir, opts.no_patch)?;

    res
}

fn match_cli_opts(opts: &CliOptions, project_dir: &PathBuf) -> Result<()> {
    match &opts.cmd {
        Command::Generate(g) => {
            let path = generate::generate_klee_tests(g, &project_dir)
                .context("Failed to execute generate command")?;
            println!("{:#?}", path);
        }
        Command::Analyze(a) => {
            analysis::analyze(a).context("Failed to execute analyze command")?;
        }
        Command::Flash(f) => {
            let path = flash::flash_to_target(f, &project_dir)
                .context("Failed to execute flash command")?;
            println!("{:#?}", path);
        }
        Command::Test(_) => (),
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
