use crate::config::RaukConfig;
use serde::Deserialize;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct CliOptions {
    /// Path to a rauk config
    #[structopt(parse(from_os_str))]
    pub config: Option<PathBuf>,
    #[structopt(subcommand)]
    pub cmd: Command,
}

#[derive(Debug, StructOpt)]
pub enum Command {
    Generate(Generation),
    Flash(Flashing),
    Analyze(Analysis),
    All(All),
}

/// Generate test vectors for an RTIC application
#[derive(Debug, StructOpt, Deserialize)]
pub struct Generation {
    /// Path to the RTIC project. Defaults to the current directory.
    #[structopt(short, long, parse(from_os_str))]
    pub path: Option<PathBuf>,
    /// Generate test for a binary target.
    #[structopt(short, long, required_unless = "example", conflicts_with = "example")]
    pub bin: Option<String>,
    /// Generate test for an example.
    #[structopt(short, long, required_unless = "bin", conflicts_with = "bin")]
    pub example: Option<String>,
    /// Generate tests in release mode.
    #[structopt(short, long)]
    pub release: bool,
}

/// Flashes a binary to the target platform, modified to allow Rauk analysis
#[derive(Debug, StructOpt, Deserialize)]
pub struct Flashing {
    /// Path to the RTIC project. Defaults to the current directory.
    #[structopt(short, long, parse(from_os_str))]
    pub path: Option<PathBuf>,
    /// Name of the binary target to flash.
    #[structopt(short, long, required_unless = "example", conflicts_with = "example")]
    pub bin: Option<String>,
    /// Name of the example to flash.
    #[structopt(short, long, required_unless = "bin", conflicts_with = "bin")]
    pub example: Option<String>,
    /// Build executable in release mode.
    #[structopt(short, long)]
    pub release: bool,
    // The target architecture to build the executable for.
    #[structopt(short, long)]
    pub target: Option<String>,
    // The name of the chip to flash to.
    #[structopt(short, long)]
    pub chip: String,
}

/// Runs the WCET analysis on the flashed binary
#[derive(Debug, StructOpt, Deserialize)]
pub struct Analysis {
    /// Path to the RTIC project. Defaults to the current directory.
    #[structopt(short, long, parse(from_os_str))]
    pub path: Option<PathBuf>,
    /// Path to DWARF.
    #[structopt(short, long, parse(from_os_str))]
    pub dwarf: PathBuf,
    /// Path to KLEE tests.
    #[structopt(short, long, parse(from_os_str))]
    pub ktests: PathBuf,
    // The name of the chip to flash to.
    #[structopt(short, long)]
    pub chip: String,
}

/// Runs all commands in one go.
#[derive(Debug, StructOpt)]
pub struct All {
    /// Path to the RTIC project. Defaults to the current directory.
    #[structopt(short, long, parse(from_os_str))]
    pub path: Option<PathBuf>,
    /// Name of the binary target to flash.
    #[structopt(short, long, required_unless = "example", conflicts_with = "example")]
    pub bin: Option<String>,
    /// Name of the example to flash.
    #[structopt(short, long, required_unless = "bin", conflicts_with = "bin")]
    pub example: Option<String>,
    /// Build executable in release mode.
    #[structopt(short, long)]
    pub release: bool,
    // The target architecture to build the executable for.
    #[structopt(short, long)]
    pub target: Option<String>,
    // The name of the chip to flash to.
    #[structopt(short, long)]
    pub chip: String,
}

trait Config {
    // Update command options with missing values if they exist in the config.
    fn update_with(&mut self, config: &RaukConfig);
}

impl Config for Generation {
    fn update_with(&mut self, config: &RaukConfig) {
        if let Some(g) = &config.generation {
            if self.path.is_none() && g.path.is_some() {
                self.path = g.path.clone();
            }
            if self.bin.is_none() && g.bin.is_some() {
                self.bin = g.bin.clone();
            } else if self.example.is_none() && g.example.is_some() {
                self.example = g.example.clone();
            }
        }
    }
}

impl Config for Flashing {
    fn update_with(&mut self, config: &RaukConfig) {
        if let Some(f) = &config.flashing {
            if self.path.is_none() && f.path.is_some() {
                self.path = f.path.clone();
            }
            if self.bin.is_none() && f.bin.is_some() {
                self.bin = f.bin.clone();
            } else if self.example.is_none() && f.example.is_some() {
                self.example = f.example.clone();
            }
            if self.target.is_none() && f.path.is_some() {
                self.target = f.target.clone();
            }
        }
    }
}

impl Config for Analysis {
    fn update_with(&mut self, config: &RaukConfig) {
        if let Some(a) = &config.analysis {
            if self.path.is_none() && a.path.is_some() {
                self.path = a.path.clone();
            }
        }
    }
}

pub fn get_cli_opts() -> CliOptions {
    CliOptions::from_args()
}
