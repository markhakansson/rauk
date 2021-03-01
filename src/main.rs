mod cli;
mod klee;
//mod target;
mod replay;
mod utils;

use cli::Command;

fn main() {
    let opts = cli::get_cli_opts();

    match opts.cmd {
        Command::Generate(g) => {
            let ktests = klee::generate_klee_tests(g);

            // Print ktests
            for test in &ktests {
                println!("{:#?}", test);
            }
        }
        Command::Analyze(a) => {
            replay::analyze(a);
        }
        _ => (),
    }
}
