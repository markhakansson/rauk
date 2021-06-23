use anyhow::Result;
use simplelog::*;
use std::fs::File;
use std::path::PathBuf;

use crate::metadata;

pub const RAUK_LOG_FILE: &str = "rauk.log";

/// Initializes a terminal and file logger
pub fn init_logger(project_dir: &PathBuf, verbose: bool) -> Result<()> {
    let mut log_output = project_dir.clone();
    log_output.push(metadata::RAUK_OUTPUT_DIR);
    let _ = std::fs::create_dir_all(&log_output);
    log_output.push(RAUK_LOG_FILE);

    let log_level = match verbose {
        true => LevelFilter::Info,
        false => LevelFilter::Warn,
    };

    CombinedLogger::init(vec![
        TermLogger::new(
            log_level,
            Config::default(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        ),
        WriteLogger::new(
            LevelFilter::Warn,
            Config::default(),
            File::create(log_output).unwrap(),
        ),
    ])?;

    Ok(())
}
