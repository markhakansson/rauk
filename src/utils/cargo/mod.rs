use anyhow::{Context, Result};
use cargo_toml::Manifest;
use std::fs::{copy, write};
use std::path::PathBuf;
use toml;

pub const RAUK_CARGO_TOML: &str = ".rauk_cargo.toml";
pub const ORIGINAL_CARGO_COPY: &str = ".Cargo.toml.copy";

/// Saves a copy of the orignal Cargo.toml in the project directory.
pub fn backup_original_cargo_toml(project_dir: &PathBuf) -> Result<()> {
    let (cargo_path, backup_path) = get_cargo_and_backup_path(project_dir);
    copy(&cargo_path, &backup_path)
        .with_context(|| format!("Could not backup {:?} to {:?}", cargo_path, backup_path))?;
    Ok(())
}

fn get_cargo_and_backup_path(project_dir: &PathBuf) -> (PathBuf, PathBuf) {
    let mut cargo_path = project_dir.clone();
    cargo_path.push("Cargo.toml");
    let mut backup_path = project_dir.clone();
    backup_path.push(ORIGINAL_CARGO_COPY);
    (cargo_path, backup_path)
}

fn get_cargo_and_rauk_path(project_dir: &PathBuf) -> (PathBuf, PathBuf) {
    let mut cargo_path = project_dir.clone();
    cargo_path.push("Cargo.toml");
    let mut rauk_path = project_dir.clone();
    rauk_path.push(RAUK_CARGO_TOML);
    (cargo_path, rauk_path)
}

/// Restores the copy of the original Cargo.toml in the project directory.
pub fn restore_orignal_cargo_toml(project_dir: &PathBuf) -> Result<()> {
    let (cargo_path, backup_path) = get_cargo_and_backup_path(project_dir);
    copy(&backup_path, &cargo_path).with_context(|| {
        format!(
            "Could not restore backup from {:?} to {:?}",
            backup_path, cargo_path
        )
    })?;
    Ok(())
}

/// Updates the custom patched rauk configuration inside the project `path`
/// If no such configuration exists it will create a new one.
pub fn update_custom_cargo_toml(project_dir: &PathBuf) -> Result<()> {
    let mut rauk_path = project_dir.clone();
    rauk_path.push(RAUK_CARGO_TOML);

    let mut cargo_path = project_dir.clone();
    cargo_path.push("Cargo.toml");

    let mut user_manifest_copy = Manifest::from_path(&cargo_path)?;
    let template = read_rauk_patch_template()?;
    patch_rauk_cargo_toml(&mut user_manifest_copy, &template);

    let toml_output = toml::to_string(&user_manifest_copy)?;
    write(rauk_path, toml_output)?;

    Ok(())
}

/// Swaps the Cargo.toml with .rauk_cargo.toml
pub fn change_cargo_toml_to_custom(project_dir: &PathBuf) -> Result<()> {
    let (cargo_path, rauk_path) = get_cargo_and_rauk_path(&project_dir);
    copy(rauk_path, cargo_path).context("Could not swap Cargo.toml with custom one.")?;
    Ok(())
}

/// Reads the template file provided by RAUK
fn read_rauk_patch_template() -> Result<Manifest> {
    let content = include_str!("template.toml");
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
