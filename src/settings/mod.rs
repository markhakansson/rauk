use crate::cli::{Analysis, Flashing, Generation};
use anyhow::Result;
use serde::Deserialize;
use std::path::PathBuf;
use std::{fs::File, io::Read};
use toml;

pub const RAUK_CONFIG_TOML: &str = "rauk.toml";

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct General {
    #[serde(default)]
    pub no_patch: Option<bool>,
    #[serde(default)]
    pub chip: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct RaukSettings {
    #[serde(default)]
    pub general: Option<General>,
}

impl RaukSettings {
    pub fn new() -> Self {
        RaukSettings { general: None }
    }
}

/// Check if the settings file exists in the project directory.
pub fn settings_file_exists(project_dir: &PathBuf) -> bool {
    let mut rauk_config_path = project_dir.clone();
    rauk_config_path.push(RAUK_CONFIG_TOML);
    rauk_config_path.exists()
}

/// Load settings from project directory if it exists.
pub fn load_settings_from_dir(project_dir: &PathBuf) -> Result<RaukSettings> {
    let mut rauk_config_path = project_dir.clone();
    rauk_config_path.push(RAUK_CONFIG_TOML);
    let mut file = File::open(rauk_config_path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    let settings: RaukSettings = toml::from_str(&contents)?;
    Ok(settings)
}
