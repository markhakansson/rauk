use anyhow::Result;
use gimli::{
    read::{
        AttributeValue, DebuggingInformationEntry, Dwarf, EndianSlice, EvaluationResult, Location,
        Unit,
    },
    RunTimeEndian, UnitHeader,
};
use object::{Object, ObjectSection};
use std::collections::HashMap;
use std::path::PathBuf;
use std::{borrow, fs};

// Details about a resource object and its location in RAM
#[derive(Debug)]
struct ObjectLocation {
    /// The name of the object.
    pub name: String,
    /// The address location of the object.
    pub address: Option<u64>,
}

pub type ObjectLocationMap = HashMap<String, Option<u64>>;

/// Loads a DWARF object from file
pub fn load_dwarf_from_file(object: object::File) -> Result<Dwarf<borrow::Cow<[u8]>>> {
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
    Ok(gimli::Dwarf::load(&load_section, &load_section_sup)?)
}

/// Reads the binary's DWARF format and returns a list of replay variables and their memory
/// location addresses.
pub fn get_replay_addresses(
    dwarf: &Dwarf<EndianSlice<RunTimeEndian>>,
) -> Result<ObjectLocationMap> {
    let mut objects: ObjectLocationMap = HashMap::new();
    // Iterate over the compilation units.
    let mut iter = dwarf.units();
    while let Some(header) = iter.next()? {
        let unit = dwarf.unit(header)?;
        let entries = parse_variable_entries(&dwarf, header, unit)?;
        for entry in entries {
            objects.insert(entry.name, entry.address);
        }
    }
    Ok(objects)
}

fn parse_variable_entries(
    dwarf: &Dwarf<EndianSlice<RunTimeEndian>>,
    header: UnitHeader<EndianSlice<RunTimeEndian>>,
    unit: Unit<EndianSlice<RunTimeEndian>>,
) -> Result<Vec<ObjectLocation>> {
    let mut objects: Vec<ObjectLocation> = vec![];
    // Iterate over the Debugging Information Entries (DIEs) in the unit.
    let mut entries = unit.entries();
    while let Some((_, entry)) = entries.next_dfs()? {
        // Iterate over the variables in the DIE.
        if entry.tag() == gimli::DW_TAG_variable {
            match parse_variable_entry(&entry, &dwarf, &header)? {
                Some(variable) => objects.push(variable),
                None => (),
            }
        }
    }
    Ok(objects)
}

fn parse_variable_entry(
    entry: &DebuggingInformationEntry<EndianSlice<RunTimeEndian>>,
    dwarf: &Dwarf<EndianSlice<RunTimeEndian>>,
    header: &UnitHeader<EndianSlice<RunTimeEndian>>,
) -> Result<Option<ObjectLocation>> {
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
                    let mut result = eval.evaluate()?;
                    match result {
                        EvaluationResult::RequiresRelocatedAddress(u) => {
                            result = eval.resume_with_relocated_address(u)?;
                        }
                        _ => (),
                    }

                    if result == EvaluationResult::Complete {
                        let eval = eval.result();
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
        let replay = ObjectLocation {
            name: name,
            address: location,
        };
        return Ok(Some(replay));
    }
    Ok(None)
}

pub fn test_subprograms(dwarf: &Dwarf<EndianSlice<RunTimeEndian>>) -> Result<()> {
    let mut iter = dwarf.units();
    while let Some(header) = iter.next()? {
        println!(
            "Unit at <.debug_info+0x{:x}>",
            header.offset().as_debug_info_offset().unwrap().0
        );
        let unit = dwarf.unit(header)?;
        parse_subprograms(dwarf, header, unit)?;
    }
    Ok(())
}

fn parse_subprograms(
    dwarf: &Dwarf<EndianSlice<RunTimeEndian>>,
    header: UnitHeader<EndianSlice<RunTimeEndian>>,
    unit: Unit<EndianSlice<RunTimeEndian>>,
) -> Result<()> {
    let mut entries = unit.entries();
    while let Some((_, entry)) = entries.next_dfs()? {
        if entry.tag() == gimli::DW_TAG_subprogram {
            parse_subprogram(dwarf, entry, &header)?;
        }
    }
    Ok(())
}

fn parse_subprogram(
    dwarf: &Dwarf<EndianSlice<RunTimeEndian>>,
    entry: &DebuggingInformationEntry<EndianSlice<RunTimeEndian>>,
    header: &UnitHeader<EndianSlice<RunTimeEndian>>,
) -> Result<()> {
    let mut attrs = entry.attrs();
    while let Some(attr) = attrs.next()? {
        println!("   {}: {:?}", attr.name(), attr.value());
    }
    Ok(())
}
