use glob::glob;
use ktest_parser::KTest;
use std::path::PathBuf;

/// Reads and parses the latest generated KTest binaries in the given path.
///
/// # Arguments
/// * `target_dir` - The directory where KLEE outputs its files.
pub fn parse_ktest_files(target_dir: &PathBuf) -> Vec<KTest> {
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
        let ktest = ktest_parser::parse_ktest(&data).unwrap();
        ktests.push(ktest);
    }

    ktests
}