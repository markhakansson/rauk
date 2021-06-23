use anyhow::{Context, Result};
use cargo_toml::Manifest;
use std::fs::{copy, rename, write};
use std::path::PathBuf;
use toml;

/// Name of the Rauk Cargo.toml
pub const RAUK_CARGO_TOML: &str = ".rauk_cargo.toml";
/// Name of the backup of the original Cargo.toml
pub const CARGO_TOML_BACKUP: &str = ".Cargo.toml.backup";

const CARGO_TOML: &str = "Cargo.toml";
const CARGO_LOCK: &str = "Cargo.lock";
const CARGO_LOCK_BACKUP: &str = ".Cargo.lock.backup";

struct CargoPaths {
    cargo_toml: PathBuf,
    cargo_lock: PathBuf,
    toml_backup: PathBuf,
    lock_backup: PathBuf,
    rauk_cargo_toml: PathBuf,
}

impl CargoPaths {
    fn new(project_dir: &PathBuf) -> CargoPaths {
        CargoPaths {
            cargo_toml: project_dir.join(CARGO_TOML),
            cargo_lock: project_dir.join(CARGO_LOCK),
            toml_backup: project_dir.join(CARGO_TOML_BACKUP),
            lock_backup: project_dir.join(CARGO_LOCK_BACKUP),
            rauk_cargo_toml: project_dir.join(RAUK_CARGO_TOML),
        }
    }
}

/// Saves copies of the orignal Cargo.toml and Cargo.lock files in the project directory.
///
/// * `project_dir` - The path to the RTIC project
pub fn backup_original_cargo_files(project_dir: &PathBuf) -> Result<()> {
    let paths = CargoPaths::new(project_dir);

    copy(&paths.cargo_toml, &paths.toml_backup).with_context(|| {
        format!(
            "Could not backup {:?} to {:?}",
            &paths.cargo_toml, &paths.toml_backup
        )
    })?;

    if paths.cargo_lock.exists() {
        rename(&paths.cargo_lock, &paths.lock_backup).with_context(|| {
            format!(
                "Could not backup {:?} to {:?}",
                &paths.cargo_lock, &paths.lock_backup
            )
        })?;
    }

    Ok(())
}

/// Restores copies of the original Cargo.toml and Cargo.lock files in the project directory.
///
/// * `project_dir` - The path to the RTIC project
pub fn restore_orignal_cargo_files(project_dir: &PathBuf) -> Result<()> {
    let paths = CargoPaths::new(project_dir);

    copy(&paths.toml_backup, &paths.cargo_toml).with_context(|| {
        format!(
            "Could not restore backup from {:?} to {:?}",
            &paths.toml_backup, &paths.cargo_toml
        )
    })?;

    if paths.cargo_lock.exists() && paths.lock_backup.exists() {
        copy(&paths.lock_backup, &paths.cargo_lock).with_context(|| {
            format!(
                "Could not restore backup from {:?} to {:?}",
                &paths.lock_backup, &paths.cargo_lock
            )
        })?;
    }

    Ok(())
}

/// Updates the custom patched rauk configuration inside the project `path`
/// If no such configuration exists it will create a new one.
///
/// * `project_dir` - The path to the RTIC project
pub fn update_custom_cargo_toml(project_dir: &PathBuf) -> Result<()> {
    let mut rauk_path = project_dir.clone();
    rauk_path.push(RAUK_CARGO_TOML);

    let mut cargo_path = project_dir.clone();
    cargo_path.push(CARGO_TOML);

    let mut user_manifest_copy = Manifest::from_path(&cargo_path)?;
    let template = read_rauk_patch_template()?;
    patch_rauk_cargo_toml(&mut user_manifest_copy, &template);

    let toml_output = toml::to_string(&user_manifest_copy)?;
    write(rauk_path, toml_output)?;

    Ok(())
}

/// Swaps the Cargo.toml with .rauk_cargo.toml
///
/// * `project_dir` - The path to the RTIC project
pub fn change_cargo_toml_to_custom(project_dir: &PathBuf) -> Result<()> {
    let paths = CargoPaths::new(project_dir);
    copy(&paths.rauk_cargo_toml, &paths.cargo_toml)
        .context("Could not swap Cargo.toml with custom one.")?;
    Ok(())
}

/// Reads the template file provided by RAUK
fn read_rauk_patch_template() -> Result<Manifest> {
    let content = include_str!("templates/v0_6.toml");
    let manifest: Manifest = toml::from_str(&content)?;
    Ok(manifest)
}

/// Patch the manifest with new dependencies, features and patches to crates.io.
fn patch_rauk_cargo_toml(manifest: &mut Manifest, patch: &Manifest) {
    for (name, dep) in patch.dependencies.iter() {
        manifest.dependencies.insert(name.clone(), dep.clone());
    }

    for (name, features) in patch.features.iter() {
        manifest.features.insert(name.clone(), features.clone());
    }

    for (name, patch) in patch.patch.iter() {
        manifest.patch.insert(name.clone(), patch.clone());
    }
}
