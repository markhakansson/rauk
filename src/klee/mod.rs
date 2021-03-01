use crate::cli::TestGeneration;
use glob::glob;
use ktest_parser::{self, KTest};
use std::process::{Command, ExitStatus, Stdio};
use std::{path::PathBuf, process::Output};

/// Builds the binary for KLEE generation. Then generates KLEE tests, parses them
/// and returns a KTest struct.
pub fn generate_klee_tests(tg: TestGeneration) -> Vec<KTest> {
    let project_dir = match tg.path.clone() {
        Some(path) => path,
        None => PathBuf::from("./"),
    };
    let mut target_dir = project_dir.clone();
    let mut cargo_path = project_dir.clone();
    let mut project_name: String = String::from("");
    target_dir.push("target/");
    cargo_path.push("Cargo.toml");

    // Build the project
    build_project(&tg, &mut cargo_path, &mut target_dir, &mut project_name)
        .expect("Could not build the project");

    let ll = fetch_latest_ll_file(&mut target_dir, &mut project_name);

    // Run KLEE
    let mut klee = Command::new("klee");
    klee.arg("--emit-all-errors").arg(ll.unwrap());
    klee.stdout(Stdio::null())
        .status()
        .expect("Could not run KLEE");

    // Fetch ktests
    read_ktest_files(&target_dir)
}

/// Builds the project in the target directory
fn build_project(
    tg: &TestGeneration,
    cargo_path: &mut PathBuf,
    target_dir: &mut PathBuf,
    project_name: &mut String,
) -> Result<ExitStatus, std::io::Error> {
    let mut cargo = Command::new("cargo");
    cargo.arg("rustc").arg("-v");

    if tg.release {
        cargo.arg("--release");
        target_dir.push("release/");
    } else {
        target_dir.push("debug/");
    }

    if tg.example.is_none() {
        *project_name = tg.bin.as_ref().unwrap().to_string();
        cargo.args(&["--bin", project_name]);
        target_dir.push("deps/");
    } else {
        *project_name = tg.example.as_ref().unwrap().to_string();
        cargo.args(&["--example", project_name]);
        target_dir.push("examples/");
    }

    cargo
        .args(&["--features", "klee-analysis"])
        .args(&["--manifest-path", cargo_path.to_str().unwrap()])
        //.arg("--message-format=json")
        .arg("--")
        // ignore linking
        .args(&["-C", "linker=true"])
        // force LTO, to get a single oject file
        .args(&["-C", "lto"])
        // output the LLVM-IR (.ll file) for KLEE analysis
        .arg("--emit=llvm-ir")
        // force panic=abort in all crates, override .cargo settings
        .env("RUSTFLAGS", "-C panic=abort");

    cargo.status()
}

fn fetch_latest_ll_file(target_dir: &mut PathBuf, project_name: &mut String) -> Option<PathBuf> {
    let glob_path =
        target_dir.clone().to_str().unwrap().to_owned() + &project_name.replace("-", "_") + "*.ll";
    let ll_glob = glob(glob_path.as_str()).expect("Failed to read glob pattern");
    let mut ll = None;
    for path in ll_glob {
        match path {
            Ok(p) => {
                if ll.is_none() {
                    ll = Some(p);
                } else {
                    let md = p.metadata().unwrap();
                    let ll_md = ll.clone().unwrap().metadata().unwrap();
                    if ll_md.accessed().unwrap() > md.accessed().unwrap() {
                        ll = Some(p);
                    }
                }
            }
            _ => (),
        }
    }

    ll
}

fn read_ktest_files(target_dir: &PathBuf) -> Vec<KTest> {
    let mut klee_last = target_dir.clone();
    klee_last.push("klee-last/");
    let ktest_pattern = klee_last.to_str().unwrap().to_owned() + "*.ktest";
    let mut ktest_paths: Vec<PathBuf> = Vec::new();
    let klee_glob = glob(ktest_pattern.as_str()).expect("Failed to read glob pattern");
    for path in klee_glob {
        match path {
            Ok(p) => ktest_paths.push(p),
            _ => (),
        }
    }

    // Convert ktests to struct
    let mut ktests: Vec<KTest> = Vec::new();
    for path in ktest_paths {
        let data = std::fs::read(path).unwrap();
        let ktest = ktest_parser::parse(&data).unwrap();
        ktests.push(ktest);
    }

    ktests
}
