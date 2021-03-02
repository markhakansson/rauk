use gimli::{
    read::{AttributeValue, Dwarf, EndianSlice, EvaluationResult, Location, Unit},
    LittleEndian, RunTimeEndian, UnitHeader,
};
use object::{Object, ObjectSection};
use std::path::PathBuf;
use std::{borrow, fs};

#[derive(Debug)]
pub struct ReplayObjectAtAddress {
    /// The name of the object.
    pub name: String,
    /// The address location of the object.
    pub address: Option<u64>,
}

/// Reads the binary's DWARF format and returns a list of replay variables and their memory
/// location addresses.
pub fn get_replay_addresses(
    binary_path: PathBuf,
) -> Result<Vec<ReplayObjectAtAddress>, gimli::Error> {
    let file = fs::File::open(&binary_path).unwrap();
    let mmap = unsafe { memmap::Mmap::map(&file).unwrap() };
    let object = object::File::parse(&*mmap).unwrap();
    let endian = if object.is_little_endian() {
        gimli::RunTimeEndian::Little
    } else {
        gimli::RunTimeEndian::Big
    };
    let mut objects: Vec<ReplayObjectAtAddress> = vec![];
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
        let unit = dwarf.unit(header)?;

        // Iterate over the Debugging Information Entries (DIEs) in the unit.
        let mut entries = unit.entries();
        while let Some((_, entry)) = entries.next_dfs()? {
            // Iterate over the attributes in the DIE.
            if entry.tag() == gimli::DW_TAG_variable {
                //println!("<{}><{:x}> {}", depth, entry.offset().0, entry.tag());
                let mut attrs = entry.attrs();
                let mut name: String = String::new();
                let mut location: Option<u64> = None;
                while let Some(attr) = attrs.next()? {
                    if attr.name() == gimli::constants::DW_AT_name {
                        match attr.value() {
                            AttributeValue::DebugStrRef(offset) => {
                                name = dwarf
                                    .string(offset)
                                    .unwrap()
                                    .to_string()
                                    .unwrap()
                                    .to_string();
                            }
                            _ => (),
                        }
                    } else if attr.name() == gimli::constants::DW_AT_location {
                        match attr.value() {
                            AttributeValue::Exprloc(e) => {
                                let mut eval = e.evaluation(header.encoding());
                                let mut result = eval.evaluate().unwrap();
                                match result {
                                    EvaluationResult::RequiresRelocatedAddress(u) => {
                                        result = eval.resume_with_relocated_address(u).unwrap();
                                    }
                                    _ => (),
                                }

                                if result == EvaluationResult::Complete {
                                    let mut eval = eval.result();
                                    let loc = eval.first().unwrap().location;
                                    match loc {
                                        Location::Address { address: a } => location = Some(a),
                                        _ => (),
                                    }
                                }
                            }
                            _ => (),
                        }
                    // Ignore external objects
                    } else if attr.name() == gimli::constants::DW_AT_external {
                        break;
                    }
                }
                if location.is_some() {
                    let replay = ReplayObjectAtAddress {
                        name: name,
                        address: location,
                    };
                    objects.push(replay);
                }
            }
        }
    }
    Ok(objects)
}

fn parse_entries(
    dwarf: &Dwarf<EndianSlice<RunTimeEndian>>,
    header: UnitHeader<EndianSlice<RunTimeEndian>>,
    unit: Unit<EndianSlice<RunTimeEndian>>,
) -> Result<Vec<ReplayObjectAtAddress>, gimli::Error> {
    let mut objects: Vec<ReplayObjectAtAddress> = vec![];
    // Iterate over the Debugging Information Entries (DIEs) in the unit.
    let mut entries = unit.entries();
    while let Some((_, entry)) = entries.next_dfs()? {
        // Iterate over the attributes in the DIE.
        if entry.tag() == gimli::DW_TAG_variable {
            //println!("<{}><{:x}> {}", depth, entry.offset().0, entry.tag());
            let mut attrs = entry.attrs();
            let mut name: String = String::new();
            let mut location: Option<u64> = None;
            while let Some(attr) = attrs.next()? {
                if attr.name() == gimli::constants::DW_AT_name {
                    match attr.value() {
                        AttributeValue::DebugStrRef(offset) => {
                            name = dwarf
                                .string(offset)
                                .unwrap()
                                .to_string()
                                .unwrap()
                                .to_string();
                        }
                        _ => (),
                    }
                } else if attr.name() == gimli::constants::DW_AT_location {
                    match attr.value() {
                        AttributeValue::Exprloc(e) => {
                            let mut eval = e.evaluation(header.encoding());
                            let mut result = eval.evaluate().unwrap();
                            match result {
                                EvaluationResult::RequiresRelocatedAddress(u) => {
                                    result = eval.resume_with_relocated_address(u).unwrap();
                                }
                                _ => (),
                            }

                            if result == EvaluationResult::Complete {
                                let mut eval = eval.result();
                                let loc = eval.first().unwrap().location;
                                match loc {
                                    Location::Address { address: a } => location = Some(a),
                                    _ => (),
                                }
                            }
                        }
                        _ => (),
                    }
                // Ignore external objects
                } else if attr.name() == gimli::constants::DW_AT_external {
                    break;
                }
            }
            if location.is_some() {
                let replay = ReplayObjectAtAddress {
                    name: name,
                    address: location,
                };
                objects.push(replay);
            }
        }
    }
    Ok(objects)
}
