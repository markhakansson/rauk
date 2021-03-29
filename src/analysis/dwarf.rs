use anyhow::Result;
use gimli::{
    read::{
        AttributeValue, DebuggingInformationEntry, Dwarf, EndianSlice, EvaluationResult, Location,
        Unit,
    },
    RunTimeEndian, UnitHeader,
};
use object::{Object, ObjectSection};
use regex::Regex;
use rustc_demangle::demangle;
use std::collections::HashMap;
use std::path::PathBuf;
use std::{borrow, fs};

pub type ObjectLocationMap = HashMap<String, Option<u64>>;

#[derive(Debug, Clone)]
pub struct Subroutine {
    pub name: String,
    pub low_pc: u64,
    pub high_pc: u64,
}

impl Subroutine {
    fn in_range(&self, address: u64) -> bool {
        (self.low_pc <= address) && (address <= self.high_pc)
    }
}

// Details about a resource object and its location in RAM
#[derive(Debug, Clone)]
pub struct ObjectLocation {
    /// The name of the object.
    pub name: String,
    /// The address location of the object.
    pub address: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct Subprogram {
    pub name: String,
    pub low_pc: u64,
    pub high_pc: u64,
}

impl Subprogram {
    fn in_range(&self, address: u64) -> bool {
        (self.low_pc <= address) && (address <= self.high_pc)
    }
}

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
        let entries = parse_variable_entries(&dwarf, &unit, &header)?;
        for entry in entries {
            objects.insert(entry.name, entry.address);
        }
    }
    Ok(objects)
}

fn parse_variable_entries(
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

fn parse_object_location(
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

/// Reads the DWARF and returns a list of all subprograms in it.
pub fn get_subprograms(dwarf: &Dwarf<EndianSlice<RunTimeEndian>>) -> Result<Vec<Subprogram>> {
    let mut iter = dwarf.units();
    let mut programs: Vec<Subprogram> = vec![];
    while let Some(header) = iter.next()? {
        let unit = dwarf.unit(header)?;
        let mut result = parse_subprograms(dwarf, &unit)?;
        programs.append(&mut result);
    }
    Ok(programs)
}

/// Returns a new list of the subprograms where the given address is in range.
pub fn get_subprograms_in_range(
    subprograms: &Vec<Subprogram>,
    address: u64,
) -> Result<Vec<Subprogram>> {
    let mut ok: Vec<Subprogram> = vec![];

    for subprogram in subprograms {
        if subprogram.in_range(address) {
            ok.push(subprogram.clone());
        }
    }

    Ok(ok)
}

pub fn get_subroutines_in_range(
    subroutines: &Vec<Subroutine>,
    address: u64,
) -> Result<Vec<Subroutine>> {
    let mut ok: Vec<Subroutine> = vec![];

    for subroutine in subroutines {
        if subroutine.in_range(address) {
            ok.push(subroutine.clone());
        }
    }

    Ok(ok)
}

/// Returns the subprogram in the given list with the shortest range.
pub fn get_shortest_range_subprogram(
    subprograms_in_range: &Vec<Subprogram>,
) -> Result<Option<Subprogram>> {
    let mut ok: Option<Subprogram> = None;
    let mut shortest_range: u64 = u64::MAX;

    for subprogram in subprograms_in_range {
        let sp_range = subprogram.high_pc - subprogram.low_pc;
        if sp_range < shortest_range {
            shortest_range = sp_range;
            ok = Some(subprogram.clone());
        }
    }
    Ok(ok)
}

fn parse_subprograms(
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

fn parse_subprogram(
    dwarf: &Dwarf<EndianSlice<RunTimeEndian>>,
    entry: &DebuggingInformationEntry<EndianSlice<RunTimeEndian>>,
) -> Result<Option<Subprogram>> {
    let mut attrs = entry.attrs();

    let mut subprogram: Option<Subprogram> = None;
    let mut name: String = String::new();
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
                    name = dwarf
                        .string(offset)
                        .unwrap()
                        .to_string()
                        .unwrap()
                        .to_string();
                }
                _ => (),
            }
        }
    }

    match (low_pc, high_pc) {
        (Some(low), Some(high)) => {
            subprogram = Some(Subprogram {
                name: name,
                low_pc: low,
                high_pc: low + high,
            })
        }
        _ => (),
    }

    Ok(subprogram)
}

/// Reads the DWARF and returns a list of subroutines and their low and high PCs.
pub fn get_subroutines(dwarf: &Dwarf<EndianSlice<RunTimeEndian>>) -> Result<Vec<Subroutine>> {
    let mut iter = dwarf.units();
    let mut subroutines: Vec<Subroutine> = Vec::new();

    let re = Regex::new(r"<impl rtic_core::Mutex for (.*?)>::lock")?;

    while let Some(header) = iter.next()? {
        let unit = dwarf.unit(header)?;
        let mut result = parse_inlined_subroutines(dwarf, &unit, &header)?;
        subroutines.append(&mut result);
    }
    Ok(subroutines)
}

/// Returns the subprogram in the given list with the shortest range.
pub fn get_shortest_range_subroutine(
    subroutines_in_range: &Vec<Subroutine>,
) -> Result<Option<Subroutine>> {
    let mut ok: Option<Subroutine> = None;
    let mut shortest_range: u64 = u64::MAX;

    for subroutine in subroutines_in_range {
        let sp_range = subroutine.high_pc - subroutine.low_pc;
        if sp_range < shortest_range {
            shortest_range = sp_range;
            ok = Some(subroutine.clone());
        }
    }
    Ok(ok)
}

fn parse_inlined_subroutines(
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

fn parse_inlined_subroutine(
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
                    let origin_name = parse_abstract_origin(dwarf, header, &origin)?;
                    name = parse_resource_name_from_abstract(origin_name);
                    // name = function to split ehre
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
            name: name,
            low_pc: low,
            high_pc: low + high,
        }),
        _ => None,
    };

    Ok(subroutine)
}

fn parse_resource_name_from_abstract(unmangled_name: String) -> Option<String> {
    let mut v: Vec<&str> = unmangled_name.split("impl rtic_core::Mutex for ").collect();
    println!("v {:?}", &v);
    if v.len() > 1 {
        match v.pop() {
            Some(string) => {
                let newsubstr: Vec<&str> = string.split(">::lock").collect();
                println!("newsubstr: {:?}", &newsubstr);
                if newsubstr.is_empty() {
                    None
                } else {
                    Some(newsubstr[0].to_string())
                }
            }
            None => None,
        }
    } else {
        None
    }
}

fn parse_abstract_origin(
    dwarf: &Dwarf<EndianSlice<RunTimeEndian>>,
    header: &UnitHeader<EndianSlice<RunTimeEndian>>,
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

fn _parse_register_location(
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
                    println!("name: {:?}", name);
                }
                _ => (),
            }
        } else if attr.name() == gimli::constants::DW_AT_location {
            println!("attribute: {:?}", attr.value());
            match attr.value() {
                AttributeValue::Exprloc(e) => {
                    let mut eval = e.evaluation(header.encoding());
                    let mut result = eval.evaluate()?;
                    println!("result: {:?}", result);
                    match result {
                        EvaluationResult::RequiresRegister {
                            register,
                            base_type,
                        } => {}
                        _ => (),
                    }

                    if result == EvaluationResult::Complete {
                        let eval = eval.result();
                        let loc = eval.first().unwrap().location;
                        println!("location: {:?}", loc);
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
