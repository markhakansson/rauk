use super::types::{ObjectLocation, Subprogram, Subroutine};
use anyhow::{Context, Result};
use gimli::{
    read::{
        AttributeValue, DebuggingInformationEntry, Dwarf, EndianSlice, EvaluationResult, Location,
        Unit,
    },
    Expression, RunTimeEndian, UnitHeader,
};
use rustc_demangle::demangle;

const FLASH_ADDRESS_START: u64 = 0x2000_0000;

/// Parses all `DW_AT_variable`s in the current DWARF unit if there are any.
///
/// * `dwarf` -The DWARF object
/// * `unit`- The current unit
/// * `header` - The current header
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
            match parse_object_location(&unit, &entry, &dwarf, &header)? {
                Some(variable) => objects.push(variable),
                None => (),
            }
        }
    }
    Ok(objects)
}

/// Tries to find the variable information (location and name) for the
/// current entry if it is a variable.
fn parse_object_location(
    unit: &Unit<EndianSlice<RunTimeEndian>>,
    entry: &DebuggingInformationEntry<EndianSlice<RunTimeEndian>>,
    dwarf: &Dwarf<EndianSlice<RunTimeEndian>>,
    header: &UnitHeader<EndianSlice<RunTimeEndian>>,
) -> Result<Option<ObjectLocation>> {
    let mut attrs = entry.attrs();
    let mut name: String = String::new();
    let mut location: Option<u64> = None;
    'outer: while let Some(attr) = attrs.next()? {
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
                    if let Some(loc) = location_from_expr(header, e)? {
                        location = Some(loc);
                    }
                }
                AttributeValue::LocationListsRef(offset) => {
                    let mut locations = dwarf.locations(unit, offset)?;
                    while let Some(loc) = locations.next()? {
                        if let Some(loc) = location_from_expr(header, loc.data)? {
                            location = Some(loc);
                            break 'outer;
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

fn location_from_expr(
    header: &UnitHeader<EndianSlice<RunTimeEndian>>,
    expr: Expression<EndianSlice<RunTimeEndian>>,
) -> Result<Option<u64>> {
    let mut location: Option<u64> = None;
    let mut eval = expr.evaluation(header.encoding());
    let mut result = eval.evaluate()?;
    loop {
        match result {
            EvaluationResult::RequiresRelocatedAddress(u) => {
                result = eval.resume_with_relocated_address(u)?;
            }
            EvaluationResult::RequiresRegister {
                register,
                base_type: _,
            } => {
                result = eval.resume_with_register(gimli::Value::Generic(register.0.into()))?;
            }
            EvaluationResult::RequiresMemory {
                address,
                size: _,
                space: _,
                base_type: _,
            } => {
                result = eval.resume_with_memory(gimli::Value::Generic(address))?;
            }
            _ => break,
        }
    }

    if result == EvaluationResult::Complete {
        let eval = eval.result();
        let loc = eval.first().unwrap().location;
        match loc {
            Location::Address { address: a } => location = Some(a),
            Location::Value { value } => {
                let v = value.to_u64(u64::MAX)?;
                if v >= FLASH_ADDRESS_START {
                    location = Some(v);
                }
            }
            _ => (),
        }
    }

    Ok(location)
}

/// Parses the `DW_AT_subprogram`s in the current DWARF unit if there are any.
///
/// * `dwarf` - The DWARF object
/// * `unit` - The current unit
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

/// Tries to parse a `DW_TAG_subprogram` in the current DWARF entry.
/// If the current entry is not a subprogram it will simply return `None`.
fn parse_subprogram(
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

/// Parses all inlined subroutines in the current unit of the DWARF object. Tries to keep all
/// relevant subroutines and discards some only.
///
/// * `dwarf` - The DWARF object
/// * `unit`- The current unit
///
pub fn parse_inlined_subroutines(
    dwarf: &Dwarf<EndianSlice<RunTimeEndian>>,
    unit: &Unit<EndianSlice<RunTimeEndian>>,
    header: &UnitHeader<EndianSlice<RunTimeEndian>>,
) -> Result<Vec<Subroutine>> {
    let mut entries = unit.entries();
    let mut subroutines: Vec<Subroutine> = Vec::new();

    while let Some((_, entry)) = entries.next_dfs()? {
        if entry.tag() == gimli::DW_TAG_inlined_subroutine {
            match parse_inlined_subroutine(dwarf, unit, header, entry).with_context(|| {
                format!(
                    "Failed to parse the inlined subroutine of entry: {:?}",
                    &entry
                )
            })? {
                Some(subroutine) => subroutines.push(subroutine),
                None => (),
            }
        }
    }
    Ok(subroutines)
}

/// Parse a `DW_AT_inlined_subroutine` if it contains a name and an
/// address range.
fn parse_inlined_subroutine(
    dwarf: &Dwarf<EndianSlice<RunTimeEndian>>,
    unit: &Unit<EndianSlice<RunTimeEndian>>,
    header: &UnitHeader<EndianSlice<RunTimeEndian>>,
    entry: &DebuggingInformationEntry<EndianSlice<RunTimeEndian>>,
) -> Result<Option<Subroutine>> {
    let mut attrs = entry.attrs();

    let mut name: Option<String> = None;
    let mut low_pc: Option<u64> = None;
    let mut high_pc: Option<u64> = None;
    let mut ranges: Vec<(u64, u64)> = vec![];

    while let Some(attr) = attrs.next()? {
        if attr.name() == gimli::constants::DW_AT_abstract_origin {
            let abbrv = dwarf.abbreviations(header)?;
            match attr.value() {
                AttributeValue::UnitRef(ur) => {
                    let origin = header.entry(&abbrv, ur)?;
                    let origin_name = parse_abstract_origin(dwarf, &origin)
                        .context("Could not get abstract origin for subroutine")?;
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
        } else if attr.name() == gimli::constants::DW_AT_ranges {
            match attr.value() {
                AttributeValue::RangeListsRef(offset) => {
                    let mut rngs = dwarf
                        .ranges(unit, offset)
                        .context("Could not get range for subroutine")?;
                    while let Some(r) = rngs.next()? {
                        ranges.push((r.begin, r.end));
                    }
                }
                _ => (),
            }
        }
    }

    match (low_pc, high_pc) {
        (Some(low), Some(high)) => {
            ranges.push((low, low + high));
        }
        _ => (),
    }

    let subroutine = match name {
        Some(name) => {
            if ranges.is_empty() {
                None
            } else {
                Some(Subroutine { name, ranges })
            }
        }
        _ => None,
    };

    Ok(subroutine)
}

/// Get the name of a `DW_AT_abstract_origin` label. If found
/// returns the demangled name.
fn parse_abstract_origin(
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
