use serde::Deserialize;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct CliOptions {
    /// Path to the RTIC project directory. Defaults to the current directory if not specified.
    #[structopt(short, long, parse(from_os_str))]
    pub path: Option<PathBuf>,
    /// Path to a rauk config [UNSUPPORTED].
    #[structopt(parse(from_os_str))]
    pub config: Option<PathBuf>,
    /// Don't patch the project's Cargo.toml. Will break the tool unless
    /// you don't have the correct dependencies/features set!
    #[structopt(long)]
    pub no_patch: bool,
    #[structopt(subcommand)]
    pub cmd: Command,
}

#[derive(Debug, StructOpt)]
pub enum Command {
    Generate(Generation),
    Flash(Flashing),
    Analyze(Analysis),
}

/// Generate test vectors for an RTIC application
#[derive(Debug, StructOpt, Deserialize)]
pub struct Generation {
    /// Generate test for a binary target.
    #[structopt(short, long, required_unless = "example", conflicts_with = "example")]
    pub bin: Option<String>,
    /// Generate test for an example.
    #[structopt(short, long, required_unless = "bin", conflicts_with = "bin")]
    pub example: Option<String>,
    /// Generate tests in release mode.
    #[structopt(short, long)]
    pub release: bool,
    /// Emit all KLEE errors.
    #[structopt(long)]
    pub emit_all_errors: bool,
}

/// Flashes a binary to the target platform, modified to allow Rauk analysis
#[derive(Debug, StructOpt, Deserialize)]
pub struct Flashing {
    /// Name of the binary target to flash.
    #[structopt(short, long, required_unless = "example", conflicts_with = "example")]
    pub bin: Option<String>,
    /// Name of the example to flash.
    #[structopt(short, long, required_unless = "bin", conflicts_with = "bin")]
    pub example: Option<String>,
    /// Build executable in release mode.
    #[structopt(short, long)]
    pub release: bool,
    /// The target architecture to build the executable for.
    #[structopt(short, long)]
    pub target: Option<String>,
    /// The name of the chip to flash to.
    #[structopt(short, long)]
    pub chip: String,
}

/// Runs the WCET analysis on the flashed binary
#[derive(Debug, StructOpt, Deserialize)]
pub struct Analysis {
    /// Path to DWARF.
    #[structopt(short, long, parse(from_os_str))]
    pub dwarf: PathBuf,
    /// Path to KLEE tests.
    #[structopt(short, long, parse(from_os_str))]
    pub ktests: PathBuf,
    /// The name of the chip to flash to.
    #[structopt(short, long)]
    pub chip: String,
}

pub fn get_cli_opts() -> CliOptions {
    CliOptions::from_args()
}
