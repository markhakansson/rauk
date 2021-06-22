use anyhow::{anyhow, Context, Result};
use chrono::prelude::Utc;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

pub const RAUK_OUTPUT_DIR: &str = "target/rauk";
pub const RAUK_METADATA_FILE: &str = "rauk_metadata.json";

/// Information about the output from all rauk commands.
/// Used to store intermediary information between commands.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RaukMetadata {
    pub project_directory: PathBuf,
    pub rauk_output_directory: PathBuf,
    pub previous_execution: PreviousExecution,
    pub artifacts: Artifacts,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifacts {
    pub release: ArtifactType,
    pub debug: ArtifactType,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactType {
    pub bin: HashMap<String, ArtifactDetail>,
    pub examples: HashMap<String, ArtifactDetail>,
}

/// Specific details of the created artifact.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactDetail {
    pub generate_output: Option<OutputInfo>,
    pub flash_output: Option<OutputInfo>,
    pub measure_output: Option<OutputInfo>,
}

impl ArtifactDetail {
    pub fn new() -> ArtifactDetail {
        ArtifactDetail {
            generate_output: None,
            flash_output: None,
            measure_output: None,
        }
    }

    /// Return the DWARF path from metadata if it exists.
    pub fn get_dwarf_path(&self) -> Option<PathBuf> {
        match self.flash_output.as_ref() {
            Some(flash_output) => flash_output.output_path.clone(),
            None => None,
        }
    }

    /// Return KTEST path from metadata if it exists.
    pub fn get_ktest_path(&self) -> Option<PathBuf> {
        match self.generate_output.as_ref() {
            Some(generate_output) => generate_output.output_path.clone(),
            None => None,
        }
    }
}

impl RaukMetadata {
    pub fn new(project_dir: &PathBuf) -> RaukMetadata {
        RaukMetadata {
            project_directory: project_dir.clone(),
            rauk_output_directory: project_dir.join(RAUK_OUTPUT_DIR),
            previous_execution: PreviousExecution::default(),
            artifacts: Artifacts {
                release: {
                    ArtifactType {
                        bin: HashMap::new(),
                        examples: HashMap::new(),
                    }
                },
                debug: {
                    ArtifactType {
                        bin: HashMap::new(),
                        examples: HashMap::new(),
                    }
                },
            },
        }
    }

    /// Loads the output info file **if it exists** in the project path.
    /// Will overwrite all values in the current struct!
    pub fn load(&mut self) -> Result<()> {
        let info_path = get_metadata_path(&self.project_directory);

        if info_path.exists() {
            let data =
                std::fs::read_to_string(&info_path).context("Failed to read RaukMetadata")?;
            let output_info: RaukMetadata = serde_json::from_str(&data).with_context(|| {
                format!("Failed to deserialize RaukMetadata with data: {:?}", &data)
            })?;
            if !output_info.previous_execution.gracefully_terminated {
                return Err(anyhow!("Previous execution of rauk did not terminate gracefully! Please manually restore your project's Cargo.toml by comparing it with the backup. Afterwards run `rauk cleanup`before proceeding!"));
            };

            self.project_directory = output_info.project_directory;
            self.rauk_output_directory = output_info.rauk_output_directory;
            self.previous_execution = output_info.previous_execution;
            self.artifacts = output_info.artifacts;
        }

        Ok(())
    }

    /// Writes the contents of RaukMetadata to file.
    pub fn save(&self) -> Result<()> {
        let rauk_path = get_rauk_output_path(&self.project_directory);
        let _ = std::fs::create_dir_all(rauk_path);

        let info_path = get_metadata_path(&self.project_directory);
        let data = serde_json::to_string(&self)?;
        std::fs::write(info_path, data)?;

        Ok(())
    }

    /// Return the details of an artifact is it exists.
    pub fn get_artifact_detail(
        &self,
        name: &str,
        release: bool,
        example: bool,
    ) -> Option<&ArtifactDetail> {
        match (release, example) {
            (true, true) => self.artifacts.release.examples.get(name),
            (true, false) => self.artifacts.release.bin.get(name),
            (false, true) => self.artifacts.debug.examples.get(name),
            (false, false) => self.artifacts.debug.bin.get(name),
        }
    }

    /// Return the mutable details of an artifact is it exists.
    pub fn get_mut_artifact_detail(
        &mut self,
        name: &str,
        release: bool,
        example: bool,
    ) -> Option<&mut ArtifactDetail> {
        match (release, example) {
            (true, true) => self.artifacts.release.examples.get_mut(name),
            (true, false) => self.artifacts.release.bin.get_mut(name),
            (false, true) => self.artifacts.debug.examples.get_mut(name),
            (false, false) => self.artifacts.debug.bin.get_mut(name),
        }
    }

    /// Insert an artifact with a name into the metadata.
    pub fn insert(&mut self, name: &str, detail: ArtifactDetail, release: bool, example: bool) {
        let name = name.to_string();
        match (release, example) {
            (true, true) => self.artifacts.release.examples.insert(name, detail),
            (true, false) => self.artifacts.release.bin.insert(name, detail),
            (false, true) => self.artifacts.debug.examples.insert(name, detail),
            (false, false) => self.artifacts.debug.bin.insert(name, detail),
        };
    }
}

/// Information about the previously executed command.
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

#[derive(Clone, Debug, Serialize, Deserialize)]
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

/// Returns the path to rauk artifacts and outputs
pub fn get_rauk_output_path(project_dir: &Path) -> PathBuf {
    let mut out_path = PathBuf::from(&project_dir);
    out_path.push(RAUK_OUTPUT_DIR);
    out_path
}

/// Returns the path to the metadata file
pub fn get_metadata_path(project_dir: &Path) -> PathBuf {
    let mut out_path = get_rauk_output_path(&project_dir);
    out_path.push(RAUK_METADATA_FILE);
    out_path
}
