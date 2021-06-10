mod parser;
mod types;

use anyhow::{anyhow, Context, Result};
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
        let mut result = parser::parse_inlined_subroutines(dwarf, &unit, &header)
            .context("Failed to parse DW_inlined_subroutines")?;
        subroutines.append(&mut result);
    }
    Ok(subroutines)
}

/// Returns a list of subroutines where the given address is in range.
///
/// * `subroutines` - A list of subroutines
/// * `address` - The address to find subroutines within the range
pub fn get_subroutines_address_in_range(
    subroutines: &Vec<Subroutine>,
    address: u64,
) -> Result<Vec<Subroutine>> {
    let mut ok: Vec<Subroutine> = vec![];

    for subroutine in subroutines {
        // If in range, push a new subroutine copy with only that range to result
        if let Some(res) = subroutine.range_from_address(address) {
            ok.push(Subroutine {
                name: subroutine.name.clone(),
                ranges: vec![res],
            });
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
        if subroutine.ranges.is_empty() {
            return Err(anyhow!("Subroutine has no address ranges"));
        }

        let (low, high) = &subroutine.ranges[0];
        let sp_range = high - low;
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
        if let Some(resource_name) = parse_resource_name_from_lock(subroutine.name.clone()) {
            let mut copy = subroutine.clone();
            copy.name = resource_name;
            resources.push(copy);
        }
    }

    resources
}

/// Try to parse the name of the RTIC resource from its unmangled name in the DWARF format.
/// If the name is not an RTIC resource it will return `None`.
fn parse_resource_name_from_lock(unmangled_name: String) -> Option<String> {
    // Currently all resource locks implement `rtic_core::Mutex` so we search for it
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

    vcells
}
