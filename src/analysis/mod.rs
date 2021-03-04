pub mod dwarf;

use crate::cli::{Analysis, Flashing};
use crate::klee::parse_ktest_files;
use std::path::PathBuf;
use std::process::{Command, ExitStatus};

use probe_rs::{
    flashing::{download_file, Format},
    Probe,
};

pub fn analyze(a: Analysis) {
    let ktest = parse_ktest_files(&a.ktests.unwrap());
    let dwarf = dwarf::get_replay_addresses(a.dwarf.unwrap()).unwrap();
    println!("{:#x?}", ktest);
    println!("{:#x?}", dwarf);
}
