use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct CliOptions {
    /// Path to the binary to test
    #[structopt(parse(from_os_str))]
    pub path: PathBuf,
    /// Enable RTT output
    #[structopt(short, long)]
    pub rtt: bool,
    /// Run WCET analysis
    #[structopt(short, long)]
    pub wcet: bool,
    /// Enable GDB server
    #[structopt(short, long)]
    pub gdb: bool,
    #[structopt(subcommand)]
    pub cmd: Command,
}

#[derive(Debug, StructOpt)]
pub enum Command {
    Generate(TestGeneration),
    Flash(Flash),
    Analyze(Analyze),
}

/// Generate test vectors for an RTIC application
#[derive(Debug, StructOpt)]
pub struct TestGeneration {
    /// Path to the RTIC project. Defaults to the current directory.
    #[structopt(short, long, parse(from_os_str))]
    pub path: PathBuf,
    /// Generate test for a binary.
    #[structopt(short, long, required_unless = "example", conflicts_with = "example")]
    pub bin: String,
    /// Generate test for an example.
    //#[structopt(short, long, required_unless = "example", conflicts_with = "bin")]
    //pub example: String,
    /// Generate tests in release mode.
    #[structopt(short, long)]
    pub release: bool,
}

/// Flashes a binary to the target platform, modified to allow Rauk analysis
#[derive(Debug, StructOpt)]
pub struct Flash {
    #[structopt(short, long)]
    pub path: Option<PathBuf>,
}

/// Runs the WCET analysis on the flashed binary
#[derive(Debug, StructOpt)]
pub struct Analyze {
    #[structopt(short, long)]
    pub path: Option<PathBuf>,
}

pub fn get_cli_opts() -> CliOptions {
    CliOptions::from_args()
}
