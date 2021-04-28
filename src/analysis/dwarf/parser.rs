use super::types::{ObjectLocation, Subprogram, Subroutine};
use anyhow::Result;
use gimli::{
    read::{
        AttributeValue, DebuggingInformationEntry, Dwarf, EndianSlice, EvaluationResult, Location,
        Unit,
    },
    RunTimeEndian, UnitHeader,
};
use rustc_demangle::demangle;

pub fn parse_variable_entries(
    dwarf: &Dwarf<EndianSlice<RunTimeEndian>>,
    unit: &Unit<EndianSlice<RunTimeEndian>>,
    header: &UnitHeader<EndianSlice<RunTimeEndian>>,
) -> Result<Vec<ObjectLocation>> {
    let mut objects: Vec<ObjectLocation> = vec![];
    // Iterate over the Debugging Information Entries (DIEs) in the unit.
    let mut entries = unit.entries();
    while let Some((_, entry)) = entries.next_dfs()? {
        // Iterate over the variables in the DIE.
        if entry.tag() == gimli::DW_TAG_variable {
            match parse_object_location(&entry, &dwarf, &header)? {
                Some(variable) => objects.push(variable),
                None => (),
            }
        }
    }
    Ok(objects)
}

pub fn parse_object_location(
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
            name,
            address: location,
        };
        return Ok(Some(replay));
    }
    Ok(None)
}

pub fn parse_subprograms(
    dwarf: &Dwarf<EndianSlice<RunTimeEndian>>,
    unit: &Unit<EndianSlice<RunTimeEndian>>,
) -> Result<Vec<Subprogram>> {
    let mut entries = unit.entries();
    let mut programs: Vec<Subprogram> = vec![];
    while let Some((_depth, entry)) = entries.next_dfs()? {
        if entry.tag() == gimli::DW_TAG_subprogram {
            let res = parse_subprogram(dwarf, entry)?;
            match res {
                Some(program) => programs.push(program),
                None => (),
            }
        }
    }
    Ok(programs)
}

pub fn parse_subprogram(
    dwarf: &Dwarf<EndianSlice<RunTimeEndian>>,
    entry: &DebuggingInformationEntry<EndianSlice<RunTimeEndian>>,
) -> Result<Option<Subprogram>> {
    let mut attrs = entry.attrs();

    let mut subprogram: Option<Subprogram> = None;
    let mut linkage_name: String = String::from("");
    let mut name: Option<String> = None;
    let mut low_pc: Option<u64> = None;
    let mut high_pc: Option<u64> = None;

    while let Some(attr) = attrs.next()? {
        if attr.name() == gimli::constants::DW_AT_low_pc {
            match attr.value() {
                AttributeValue::Addr(a) => low_pc = Some(a),
                _ => (),
            }
        } else if attr.name() == gimli::constants::DW_AT_high_pc {
            match attr.value() {
                AttributeValue::Udata(a) => high_pc = Some(a),
                _ => (),
            }
        } else if attr.name() == gimli::constants::DW_AT_name {
            match attr.value() {
                AttributeValue::DebugStrRef(offset) => {
                    let sub_name = dwarf
                        .string(offset)
                        .unwrap()
                        .to_string()
                        .unwrap()
                        .to_string();
                    // Ignore reserved functions
                    if !sub_name.starts_with("__") {
                        name = Some(sub_name);
                    }
                }
                _ => (),
            }
        } else if attr.name() == gimli::constants::DW_AT_linkage_name {
            match attr.value() {
                AttributeValue::DebugStrRef(offset) => {
                    let sub_name = dwarf
                        .string(offset)
                        .unwrap()
                        .to_string()
                        .unwrap()
                        .to_string();
                    linkage_name = demangle(&sub_name).to_string();
                }
                _ => (),
            }
        }
    }

    match (name, low_pc, high_pc) {
        (Some(name), Some(low), Some(high)) => {
            subprogram = Some(Subprogram {
                name,
                linkage_name,
                low_pc: low,
                high_pc: low + high,
            })
        }
        _ => (),
    }

    Ok(subprogram)
}

pub fn parse_inlined_subroutines(
    dwarf: &Dwarf<EndianSlice<RunTimeEndian>>,
    unit: &Unit<EndianSlice<RunTimeEndian>>,
    header: &UnitHeader<EndianSlice<RunTimeEndian>>,
) -> Result<Vec<Subroutine>> {
    let mut entries = unit.entries();
    let mut subroutines: Vec<Subroutine> = Vec::new();

    while let Some((_, entry)) = entries.next_dfs()? {
        if entry.tag() == gimli::DW_TAG_inlined_subroutine {
            match parse_inlined_subroutine(dwarf, header, entry)? {
                Some(subroutine) => subroutines.push(subroutine),
                None => (),
            }
        }
    }
    Ok(subroutines)
}

pub fn parse_inlined_subroutine(
    dwarf: &Dwarf<EndianSlice<RunTimeEndian>>,
    header: &UnitHeader<EndianSlice<RunTimeEndian>>,
    entry: &DebuggingInformationEntry<EndianSlice<RunTimeEndian>>,
) -> Result<Option<Subroutine>> {
    let mut attrs = entry.attrs();

    let mut name: Option<String> = None;
    let mut low_pc: Option<u64> = None;
    let mut high_pc: Option<u64> = None;

    while let Some(attr) = attrs.next()? {
        if attr.name() == gimli::constants::DW_AT_abstract_origin {
            let abbrv = dwarf.abbreviations(header)?;
            match attr.value() {
                AttributeValue::UnitRef(ur) => {
                    let origin = header.entry(&abbrv, ur)?;
                    let origin_name = parse_abstract_origin(dwarf, &origin)?;
                    name = Some(origin_name);
                }
                _ => (),
            }
        } else if attr.name() == gimli::constants::DW_AT_low_pc {
            match attr.value() {
                AttributeValue::Addr(a) => low_pc = Some(a),
                _ => (),
            }
        } else if attr.name() == gimli::constants::DW_AT_high_pc {
            match attr.value() {
                AttributeValue::Udata(a) => high_pc = Some(a),
                _ => (),
            }
        }
    }

    let subroutine = match (name, low_pc, high_pc) {
        (Some(name), Some(low), Some(high)) => Some(Subroutine {
            name,
            low_pc: low,
            high_pc: low + high,
        }),
        _ => None,
    };

    Ok(subroutine)
}

pub fn parse_abstract_origin(
    dwarf: &Dwarf<EndianSlice<RunTimeEndian>>,
    entry: &DebuggingInformationEntry<EndianSlice<RunTimeEndian>>,
) -> Result<String> {
    let mut attrs = entry.attrs();
    let mut name: String = String::new();

    while let Some(attr) = attrs.next()? {
        if attr.name() == gimli::constants::DW_AT_linkage_name {
            match attr.value() {
                AttributeValue::DebugStrRef(offset) => {
                    let origin_name = dwarf
                        .string(offset)
                        .unwrap()
                        .to_string()
                        .unwrap()
                        .to_string();
                    name = demangle(&origin_name).to_string();
                }
                _ => (),
            }
        }
    }

    Ok(name)
}
