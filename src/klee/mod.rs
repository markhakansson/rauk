use crate::cli;
use anyhow;
use cargo_metadata as cm;
use ktest_parser;
use std::process::Command;

pub fn generate_klee_tests(tg: cli::TestGeneration) {
    let mut cargo = Command::new("cargo");
    cargo.arg("rustc");

    if tg.bin.is_empty() {
        cargo.args(&["--bin", tg.bin.as_str()]);
    }
    //} else {
    //    cargo.args(&["--example", tg.example.as_str()]);
    //}

    if tg.release {
        cargo.arg("--release");
    }

    let mut path = tg.path.clone();
    path.push("Cargo.toml");
    println!("{:?}", path);

    cargo
        .args(&["--features", "klee-analysis"])
        .args(&["--manifest-path", path.to_str().unwrap()])
        // enable shell coloring of result
        .arg("--color=always")
        .arg("--")
        // ignore linking
        .args(&["-C", "linker=true"])
        // force LTO, to get a single oject file
        .args(&["-C", "lto"])
        // output the LLVM-IR (.ll file) for KLEE analysis
        .arg("--emit=llvm-ir")
        // force panic=abort in all crates, override .cargo settings
        .env("RUSTFLAGS", "-C panic=abort");

    let status = cargo.status().unwrap();
    if !status.success() {
        println!("{:?}", status.code().unwrap());
    }
}
