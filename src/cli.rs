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
}

pub fn get_cli_opts() -> CliOptions {
    CliOptions::from_args()
}
