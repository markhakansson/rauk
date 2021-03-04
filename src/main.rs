mod analysis;
mod cli;
mod config;
mod flash;
mod generate;
mod klee;
mod utils;

use analysis::analyze;
use cli::Command;
use config::RaukConfig;

fn main() {
    let opts = cli::get_cli_opts();

    let config = match opts.config {
        Some(path) => {
            let config = config::load_config_from_file(&path).unwrap();
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
            let path = generate::generate_klee_tests(g);
            println!("{:#?}", path);
        }
        Command::Analyze(a) => {
            // analysis::analyze(a);
        }
        Command::Flash(f) => {
            let path = flash::flash_to_target(f);
            println!("{:#?}", path);
        }
        Command::All(a) => {
            run_all(a);
        }
    }
}

fn run_all(all: cli::All) {
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
    let klee_path = generate::generate_klee_tests(generate).unwrap();
    let dwarf_path = flash::flash_to_target(flash).unwrap();
    let analysis = cli::Analysis {
        path: all.path.clone(),
        dwarf: Some(dwarf_path),
        ktests: Some(klee_path),
    };
    analyze(analysis);
}
