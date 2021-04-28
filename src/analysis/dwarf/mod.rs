mod parser;
mod types;

use anyhow::Result;
use gimli::{
    read::{Dwarf, EndianSlice},
    RunTimeEndian,
};
use object::{Object, ObjectSection};
use std::borrow;
use std::collections::HashMap;
pub use types::{ObjectLocationMap, Subprogram, Subroutine};

/// Loads a DWARF object from file
///
/// * `object` - The file to read
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

/// Reads the binary's DWARF format and returns a map of replay variables and their memory
/// location addresses.
///
/// * `dwarf` - A DWARF object
pub fn get_replay_addresses(
    dwarf: &Dwarf<EndianSlice<RunTimeEndian>>,
) -> Result<ObjectLocationMap> {
    let mut objects: ObjectLocationMap = HashMap::new();
    // Iterate over the compilation units.
    let mut iter = dwarf.units();
    while let Some(header) = iter.next()? {
        let unit = dwarf.unit(header)?;
        let entries = parser::parse_variable_entries(&dwarf, &unit, &header)?;
        for entry in entries {
            objects.insert(entry.name, entry.address);
        }
    }
    Ok(objects)
}

/// Reads the DWARF and returns a list of all subprograms in it.
///
/// * `dwarf` - A DWARF object
/// * `ignore_reserved` - Ignore reserved subprograms starting with `__`
pub fn get_subprograms(dwarf: &Dwarf<EndianSlice<RunTimeEndian>>) -> Result<Vec<Subprogram>> {
    let mut iter = dwarf.units();
    let mut programs: Vec<Subprogram> = vec![];
    while let Some(header) = iter.next()? {
        let unit = dwarf.unit(header)?;
        let mut result = parser::parse_subprograms(dwarf, &unit)?;
        programs.append(&mut result);
    }
    Ok(programs)
}

/// Returns a new list of the subprograms where the given address is in range.
pub fn get_subprograms_address_in_range(
    subprograms: &Vec<Subprogram>,
    address: u64,
) -> Result<Vec<Subprogram>> {
    let mut ok: Vec<Subprogram> = vec![];

    for subprogram in subprograms {
        if subprogram.address_in_range(address) {
            ok.push(subprogram.clone());
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

/// Reads the DWARF and returns a list of subroutines and their low and high PCs.
///
/// * `dwarf` - A DWARF object
pub fn get_subroutines(dwarf: &Dwarf<EndianSlice<RunTimeEndian>>) -> Result<Vec<Subroutine>> {
    let mut iter = dwarf.units();
    let mut subroutines: Vec<Subroutine> = Vec::new();

    while let Some(header) = iter.next()? {
        let unit = dwarf.unit(header)?;
        let mut result = parser::parse_inlined_subroutines(dwarf, &unit, &header)?;
        subroutines.append(&mut result);
    }
    Ok(subroutines)
}

/// Returns a list of subroutines where the given address is in range.
pub fn get_subroutines_address_in_range(
    subroutines: &Vec<Subroutine>,
    address: u64,
) -> Result<Vec<Subroutine>> {
    let mut ok: Vec<Subroutine> = vec![];

    for subroutine in subroutines {
        if subroutine.address_in_range(address) {
            ok.push(subroutine.clone());
        }
    }

    Ok(ok)
}

/// Returns a list of subroutines that are within the `low_pc` to `high_pc` range.
pub fn get_subroutines_in_range(
    subroutines: &Vec<Subroutine>,
    low_pc: u64,
    high_pc: u64,
) -> Result<Vec<Subroutine>> {
    let mut ok: Vec<Subroutine> = vec![];

    for subroutine in subroutines {
        if subroutine.in_range(low_pc..high_pc) {
            ok.push(subroutine.clone());
        }
    }

    Ok(ok)
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

/// From a list of subroutines, returns a list of the subroutines that are locked resources
/// inside an RTIC task.
pub fn get_resources_from_subroutines(subroutines: &Vec<Subroutine>) -> Vec<Subroutine> {
    let mut resources: Vec<Subroutine> = Vec::new();

    for subroutine in subroutines {
        if let Some(resource_name) = parse_resource_name_from_abstract(subroutine.name.clone()) {
            let mut copy = subroutine.clone();
            copy.name = resource_name;
            resources.push(copy);
        }
    }

    resources
}

fn parse_resource_name_from_abstract(unmangled_name: String) -> Option<String> {
    let mut v: Vec<&str> = unmangled_name.split("impl rtic_core::Mutex for ").collect();
    if v.len() > 1 {
        match v.pop() {
            Some(string) => {
                let newsubstr: Vec<&str> = string.split(">::lock").collect();
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

/// From a list of subroutines, returns a list of the subroutines that are hardware
/// readings. I.e. vcell::get or vcell::as_ptr.
pub fn get_vcell_from_subroutines(subroutines: &Vec<Subroutine>) -> Vec<Subroutine> {
    let mut vcells: Vec<Subroutine> = Vec::new();

    for subroutine in subroutines {
        if subroutine.name.contains("vcell") {
            if subroutine.name.contains("get") || subroutine.name.contains("as_ptr") {
                vcells.push(subroutine.clone());
            }
        }
    }

    // This might be unneccessary but it's better to be safe than sorry
    vcells.sort_by(|a, b| a.low_pc.cmp(&b.low_pc));

    vcells
}

/// Returns a list with the Subprograms containing the `name` as a substring in its name or linkage
/// name.
pub fn get_subprograms_with_name(subprograms: &Vec<Subprogram>, name: &str) -> Vec<Subprogram> {
    let mut ok: Vec<Subprogram> = Vec::new();

    for subprogram in subprograms {
        if subprogram.name.contains(name) || subprogram.linkage_name.contains(name) {
            ok.push(subprogram.clone());
        }
    }

    ok
}
