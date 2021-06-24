use anyhow::Result;
use serde::Deserialize;
use std::path::PathBuf;
use std::{fs::File, io::Read};
use toml;

use crate::cli::{FlashInput, MeasureInput};

pub const RAUK_CONFIG_TOML: &str = "rauk.toml";

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct General {
    #[serde(default)]
    pub no_patch: Option<bool>,
    #[serde(default)]
    pub chip: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub halt_timeout: Option<u64>,
}

/// Rauk settings file that can be used instead of command input
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

impl FlashInput {
    /// If input is missing, check if it is available in the settings
    /// and overwrite the missing input with those values.
    pub fn get_missing_input(&mut self, settings: &RaukSettings) {
        if let Some(general) = &settings.general {
            if self.target.is_none() {
                self.target = general.target.clone();
            }
            if self.chip.is_none() {
                self.chip = general.chip.clone();
            }
            if self.halt_timeout.is_none() {
                self.halt_timeout = general.halt_timeout.clone();
            }
        }
    }
}

impl MeasureInput {
    /// If input is missing, check if it is available in the settings
    /// and overwrite the missing input with those values.
    pub fn get_missing_input(&mut self, settings: &RaukSettings) {
        if let Some(general) = &settings.general {
            if self.chip.is_none() {
                self.chip = general.chip.clone();
            }
            if self.halt_timeout.is_none() {
                self.halt_timeout = general.halt_timeout.clone();
            }
        }
    }
}

/// Check if the settings file exists in the project directory.
pub fn settings_file_exists(project_dir: &PathBuf) -> bool {
    let mut rauk_config_path = project_dir.clone();
    rauk_config_path.push(RAUK_CONFIG_TOML);
    rauk_config_path.exists()
}

/// Load settings from project directory if it exists.
fn load_settings_from_dir(project_dir: &PathBuf) -> Result<RaukSettings> {
    let mut rauk_config_path = project_dir.clone();
    rauk_config_path.push(RAUK_CONFIG_TOML);
    let mut file = File::open(rauk_config_path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    let settings: RaukSettings = toml::from_str(&contents)?;
    Ok(settings)
}

/// Loads settings from file if it exists, otherwise creates an empty
/// settings struct.
pub fn load_settings(project_dir: &PathBuf) -> Result<RaukSettings> {
    let settings = if settings_file_exists(&project_dir) {
        info!("Loading user settings from file");
        load_settings_from_dir(&project_dir)?
    } else {
        info!("No user settings file found");
        RaukSettings::new()
    };

    Ok(settings)
}
