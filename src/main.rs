mod cli;
mod klee;
mod target;
mod utils;

use cli::Command;

fn main() {
    let opts = cli::get_cli_opts();

    match opts.cmd {
        Command::Generate(g) => klee::generate_klee_tests(g),
        _ => (),
    }
}
