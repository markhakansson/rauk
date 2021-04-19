use anyhow::{anyhow, Context, Result};
use chrono::prelude::Utc;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const RAUK_OUTPUT_INFO: &str = ".rauk_info.json";

/// Information about the output from all rauk commands.
/// Used to store intermediary information between commands.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RaukInfo {
    pub project_directory: PathBuf,
    pub previous_execution: PreviousExecution,
    pub generate_output: Option<OutputInfo>,
    pub flash_output: Option<OutputInfo>,
    pub analyze_output: Option<OutputInfo>,
}

impl RaukInfo {
    pub fn new(project_dir: &PathBuf) -> RaukInfo {
        RaukInfo {
            project_directory: project_dir.clone(),
            previous_execution: PreviousExecution::default(),
            generate_output: None,
            flash_output: None,
            analyze_output: None,
        }
    }

    /// Loads the output info file **if it exists** in the project path.
    /// Will overwrite all values in the current struct!
    pub fn load(&mut self) -> Result<()> {
        let info_path = get_output_path(&self.project_directory);

        if info_path.exists() {
            let data =
                std::fs::read_to_string(&info_path).context("Failed to read RaukOutputInfo")?;
            let output_info: RaukInfo = serde_json::from_str(&data).with_context(|| {
                format!(
                    "Failed to deserialize RaukOutputInfo with data: {:?}",
                    &data
                )
            })?;
            if !output_info.previous_execution.gracefully_terminated {
                return Err(anyhow!("Previous execution of rauk did not execute gracefully! Please manually restore your project's Cargo.toml by comparing it with the backup before proceeding. Then run `rauk cleanup`!"));
            };

            self.project_directory = output_info.project_directory;
            self.previous_execution = output_info.previous_execution;
            self.generate_output = output_info.generate_output;
            self.flash_output = output_info.flash_output;
            self.analyze_output = output_info.analyze_output;
        }

        Ok(())
    }

    /// Writes the contents of RaukOutputInfo to file.
    pub fn save(&self) -> Result<()> {
        let info_path = get_output_path(&self.project_directory);
        let data = serde_json::to_string(&self)?;
        std::fs::write(info_path, data)?;

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviousExecution {
    pub gracefully_terminated: bool,
}

impl Default for PreviousExecution {
    fn default() -> Self {
        PreviousExecution {
            gracefully_terminated: false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputInfo {
    pub output_path: Option<PathBuf>,
    pub last_changed: Option<String>,
}

impl OutputInfo {
    pub fn new(output_path: Option<PathBuf>) -> OutputInfo {
        let time = Utc::now();

        OutputInfo {
            output_path,
            last_changed: Some(time.to_rfc3339()),
        }
    }
}

fn get_output_path(path: &Path) -> PathBuf {
    let mut out_path = PathBuf::from(&path);
    out_path.push(RAUK_OUTPUT_INFO);
    out_path
}