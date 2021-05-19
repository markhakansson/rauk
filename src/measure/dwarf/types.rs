use std::collections::HashMap;

type Name = String;
type MemoryLocation = Option<u64>;

/// A map with the name of an RTIC resource and its memory location
pub type ObjectLocationMap = HashMap<Name, MemoryLocation>;

/// A DWARF subroutine containing the useful values for Rauk analysis
#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd)]
pub struct Subroutine {
    /// The demangled name of the subroutine
    pub name: String,
    /// The starting address of this subroutine
    pub low_pc: u64,
    /// The ending address of this subroutine
    pub high_pc: u64,
}

impl Subroutine {
    /// Checks if `address` is inside this subroutine's range.
    pub fn address_in_range(&self, address: u64) -> bool {
        (self.low_pc <= address) && (address <= self.high_pc)
    }
    /// Checks if this subroutine is within `range`
    pub fn in_range(&self, range: std::ops::Range<u64>) -> bool {
        range.contains(&self.low_pc) && range.contains(&self.high_pc)
    }
}

/// Details about a resource object and its location in RAM
#[derive(Debug, Clone)]
pub struct ObjectLocation {
    /// The name of the object.
    pub name: String,
    /// The address location of the object.
    pub address: Option<u64>,
}

/// A DWARF subprogram containing the useful value for Rauk analysis
#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd)]
pub struct Subprogram {
    /// The demangled name of the subprogram
    pub name: String,
    /// The demangled linkage name of this subprogram
    pub linkage_name: String,
    /// The starting address of this subprogram
    pub low_pc: u64,
    /// The ending address of this subprogram
    pub high_pc: u64,
}

impl Subprogram {
    /// Checks if `address` is inside this subprogram's range.
    pub fn address_in_range(&self, address: u64) -> bool {
        (self.low_pc <= address) && (address <= self.high_pc)
    }
}
