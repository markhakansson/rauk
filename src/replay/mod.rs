use crate::cli::Analyze;
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

    build_replay_harness(&a, &mut cargo_path, &mut target_dir)
        .expect("Could not build the replay harness");

    println!("{:?}", target_dir);
    dwarf_check(target_dir);
}

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

fn dwarf_check(path: PathBuf) {
    let file = fs::File::open(&path).unwrap();
    let mmap = unsafe { memmap::Mmap::map(&file).unwrap() };
    let object = object::File::parse(&*mmap).unwrap();
    let endian = if object.is_little_endian() {
        gimli::RunTimeEndian::Little
    } else {
        gimli::RunTimeEndian::Big
    };
    dump_file(&object, endian).unwrap();
}

fn dump_file(object: &object::File, endian: gimli::RunTimeEndian) -> Result<(), gimli::Error> {
    // Load a section and return as `Cow<[u8]>`.
    let load_section = |id: gimli::SectionId| -> Result<borrow::Cow<[u8]>, gimli::Error> {
        match object.section_by_name(id.name()) {
            Some(ref section) => Ok(section
                .uncompressed_data()
                .unwrap_or(borrow::Cow::Borrowed(&[][..]))),
            None => Ok(borrow::Cow::Borrowed(&[][..])),
        }
    };
    // Load a supplementary section. We don't have a supplementary object file,
    // so always return an empty slice.
    let load_section_sup = |_| Ok(borrow::Cow::Borrowed(&[][..]));

    // Load all of the sections.
    let dwarf_cow = gimli::Dwarf::load(&load_section, &load_section_sup)?;

    // Borrow a `Cow<[u8]>` to create an `EndianSlice`.
    let borrow_section: &dyn for<'a> Fn(
        &'a borrow::Cow<[u8]>,
    ) -> gimli::EndianSlice<'a, gimli::RunTimeEndian> =
        &|section| gimli::EndianSlice::new(&*section, endian);

    // Create `EndianSlice`s for all of the sections.
    let dwarf = dwarf_cow.borrow(&borrow_section);

    // Iterate over the compilation units.
    let mut iter = dwarf.units();
    while let Some(header) = iter.next()? {
        println!(
            "Unit at <.debug_info+0x{:x}>",
            header.offset().as_debug_info_offset().unwrap().0
        );
        let unit = dwarf.unit(header)?;

        // Iterate over the Debugging Information Entries (DIEs) in the unit.
        let mut depth = 0;
        let mut entries = unit.entries();
        while let Some((delta_depth, entry)) = entries.next_dfs()? {
            depth += delta_depth;
            println!("<{}><{:x}> {}", depth, entry.offset().0, entry.tag());

            // Iterate over the attributes in the DIE.
            let mut attrs = entry.attrs();
            while let Some(attr) = attrs.next()? {
                println!("   {}: {:?}", attr.name(), attr.value());
            }
        }
    }
    Ok(())
}
