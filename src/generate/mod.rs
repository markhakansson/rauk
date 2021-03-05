use crate::cli::Generation;
use glob::glob;
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Stdio};

/// Builds the test harness, then generates test vectors from it using KLEE.
/// Returns the path to where KLEE generated its tests.
pub fn generate_klee_tests(tg: Generation) -> Result<PathBuf, std::io::Error> {
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
    build_test_harness(&tg, &mut cargo_path, &mut target_dir, &mut project_name)?;

    let ll = fetch_latest_ll_file(&mut target_dir, &mut project_name);

    // Run KLEE
    let mut klee = Command::new("klee");
    klee.arg("--emit-all-errors").arg(ll.unwrap());
    klee.stdout(Stdio::null()).status()?;

    Ok(target_dir)
}

/// Builds the test harness.
fn build_test_harness(
    tg: &Generation,
    cargo_path: &mut PathBuf,
    target_dir: &mut PathBuf,
    project_name: &mut String,
) -> Result<ExitStatus, std::io::Error> {
    let mut cargo = Command::new("cargo");
    cargo.arg("rustc");

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
