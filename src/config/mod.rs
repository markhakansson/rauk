use crate::cli::{Analysis, Flashing, Generation};
use anyhow::Result;
use serde::Deserialize;
use std::path::PathBuf;
use std::{fs::File, io::Read};
use toml;

#[derive(Deserialize)]
pub struct RaukConfig {
    pub analysis: Option<Analysis>,
    pub flashing: Option<Flashing>,
    pub generation: Option<Generation>,
}

// Loads a rauk configuration at path
pub fn load_config_from_file(path: &PathBuf) -> Result<RaukConfig> {
    let mut file = File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    let config: RaukConfig = toml::from_str(&contents)?;

    Ok(RaukConfig {
        analysis: config.analysis,
        flashing: config.flashing,
        generation: config.generation,
    })
}
