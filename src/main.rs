mod analysis;
mod cli;
mod config;
mod flash;
mod generate;
mod utils;

use analysis::analyze;
use anyhow::{Context, Result};
use cli::Command;
use config::RaukConfig;

fn main() -> Result<()> {
    match_cli_opts()?;
    Ok(())
}

fn match_cli_opts() -> Result<()> {
    let opts = cli::get_cli_opts();

    let config = match opts.config {
        Some(path) => {
            let config = config::load_config_from_file(&path)?;
            config
        }
        None => RaukConfig {
            analysis: None,
            flashing: None,
            generation: None,
        },
    };

    match opts.cmd {
        Command::Generate(g) => {
            let path = generate::generate_klee_tests(g)?;
            println!("{:#?}", path);
        }
        Command::Analyze(a) => {
            analysis::analyze(a)?;
        }
        Command::Flash(f) => {
            let path = flash::flash_to_target(f)?;
            println!("{:#?}", path);
        }
        Command::All(a) => {
            run_all(a)?;
        }
    }

    Ok(())
}

fn run_all(all: cli::All) -> Result<()> {
    let generate = cli::Generation {
        path: all.path.clone(),
        bin: all.bin.clone(),
        example: all.example.clone(),
        release: all.release.clone(),
    };
    let flash = cli::Flashing {
        path: all.path.clone(),
        bin: all.bin.clone(),
        example: all.example.clone(),
        release: all.release.clone(),
        target: all.target.clone(),
        chip: all.chip.clone(),
    };
    let klee_path = generate::generate_klee_tests(generate)?;
    let dwarf_path = flash::flash_to_target(flash)?;
    let analysis = cli::Analysis {
        path: all.path.clone(),
        dwarf: dwarf_path,
        ktests: klee_path,
        chip: all.chip.clone(),
        output: None,
    };
    analyze(analysis)?;

    Ok(())
}
