pub mod dwarf;

use crate::cli::Analyze;
use gimli::read::{AttributeValue, EvaluationResult, Location};
use object::{Object, ObjectSection};
use std::path::PathBuf;
use std::process::{Command, ExitStatus};
use std::{borrow, env, fs};

pub fn analyze(a: Analyze) {
    let project_dir = match a.path.clone() {
        Some(path) => path,
        None => PathBuf::from("./"),
    };
    let mut target_dir = project_dir.clone();
    let mut cargo_path = project_dir.clone();
    target_dir.push("target/");
    cargo_path.push("Cargo.toml");
    target_dir.push("thumbv7em-none-eabi/debug/test-harness");
    println!("{:?}", target_dir);
    let objects = dwarf::get_replay_addresses(target_dir);
    println!("{:#x?}", objects.unwrap());
}

// pub fn analyze(a: Analyze) {
//     let project_dir = match a.path.clone() {
//         Some(path) => path,
//         None => PathBuf::from("./"),
//     };
//     let mut target_dir = project_dir.clone();
//     let mut cargo_path = project_dir.clone();
//     target_dir.push("target/");
//     cargo_path.push("Cargo.toml");
//
//     build_replay_harness(&a, &mut cargo_path, &mut target_dir)
//         .expect("Could not build the replay harness");
//
//     println!("{:?}", target_dir);
//     dwarf_check(target_dir);
// }

fn build_replay_harness(
    a: &Analyze,
    cargo_path: &mut PathBuf,
    target_dir: &mut PathBuf,
) -> Result<ExitStatus, std::io::Error> {
    let mut cargo = Command::new("cargo");
    cargo.arg("build");

    if a.target.is_some() {
        let target = a.target.clone().unwrap();
        cargo.args(&["--target", target.as_str()]);
        target_dir.push(target);
    }

    if a.release {
        cargo.arg("--release");
        target_dir.push("release/");
    } else {
        target_dir.push("debug/");
    }

    let name: String;
    if a.example.is_none() {
        name = a.bin.as_ref().unwrap().to_string();
        cargo.args(&["--bin", name.as_str()]);
    } else {
        name = a.example.as_ref().unwrap().to_string();
        cargo.args(&["--example", name.as_str()]);
    }
    target_dir.push(name);

    cargo
        .args(&["--features", "klee-replay"])
        .args(&["--manifest-path", cargo_path.to_str().unwrap()]);

    cargo.status()
}
