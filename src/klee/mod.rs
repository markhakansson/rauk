use crate::cli;
use anyhow;
use cargo_metadata::Message;
use ktest_parser;
use std::process::{Command, Stdio};

pub fn generate_klee_tests(tg: cli::TestGeneration) {
    let mut cargo = Command::new("cargo");
    cargo.arg("rustc").arg("-v");

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
        .arg("--message-format=json")
        .arg("--")
        // ignore linking
        .args(&["-C", "linker=true"])
        // force LTO, to get a single oject file
        .args(&["-C", "lto"])
        // output the LLVM-IR (.ll file) for KLEE analysis
        .arg("--emit=llvm-ir")
        // force panic=abort in all crates, override .cargo settings
        .env("RUSTFLAGS", "-C panic=abort");
    // output the LLVM-IR (.ll file) for KLEE analysis

    let mut command = cargo.stdout(Stdio::piped()).spawn().unwrap();
    let reader = std::io::BufReader::new(command.stdout.take().unwrap());
    for message in cargo_metadata::Message::parse_stream(reader) {
        match message.unwrap() {
            Message::CompilerMessage(msg) => {
                println!("{:?}", msg);
            }
            Message::CompilerArtifact(artifact) => {
                println!("{:?}", artifact);
            }
            Message::BuildScriptExecuted(script) => {
                println!("{:?}", script);
            }
            Message::BuildFinished(finished) => {
                println!("{:?}", finished);
            }
            _ => (), // Unknown message
        }
    }

    let output = command.wait().expect("Couldn't get cargo's exit status");
    // let status = cargo.status().unwrap();

    // if !status.success() {
    //     println!("{:?}", status.code().unwrap());
    // }
}
