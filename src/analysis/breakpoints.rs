/// Information about the breakpoint type for RAUK analysis
#[derive(Debug, Clone, PartialEq)]
pub enum Breakpoint {
    Other(OtherBreakpoint),
    Entry(EntryBreakpoint),
    Exit(ExitBreakpoint),
}

impl Breakpoint {
    pub fn is_exit(&self) -> bool {
        match self {
            Breakpoint::Exit(_) => true,
            _ => false,
        }
    }
}

/// The type of the entry breakpoint for a new scope.
#[derive(Debug, Clone, PartialEq)]
pub enum EntryBreakpoint {
    HardwareTaskStart = 2,
    ResourceLockStart = 3,
    SoftwareTaskStart = 4,
}

/// The type of the exit breakpoint for a scope.
#[derive(Debug, Clone, PartialEq)]
pub enum ExitBreakpoint {
    SoftwareTaskEnd = 251,
    ResourceLockEnd = 252,
    HardwareTaskEnd = 253,
}

/// The type for breakpoints that are not part of a scope.
#[derive(Debug, Clone, PartialEq)]
pub enum OtherBreakpoint {
    /// Default breakpoints such as asm::bkpt() is always 0
    Default = 0,
    /// Inside an rtic task #[task]
    InsideTask = 1,
    /// Inside a resource closure `x.lock(|x| { //here }`
    InsideLockClosure = 5,
    /// Any other breakpoint will be labelled as invalid
    Invalid = 100,
    /// Inside the RTIC lock function
    InsideLock = 254,
    /// Replay harness starts at this breakpoint number
    ReplayStart = 255,
}

impl From<u8> for Breakpoint {
    fn from(u: u8) -> Breakpoint {
        match u {
            0 => Breakpoint::Other(OtherBreakpoint::Default),
            1 => Breakpoint::Other(OtherBreakpoint::InsideTask),
            2 => Breakpoint::Entry(EntryBreakpoint::HardwareTaskStart),
            3 => Breakpoint::Entry(EntryBreakpoint::ResourceLockStart),
            4 => Breakpoint::Entry(EntryBreakpoint::SoftwareTaskStart),
            5 => Breakpoint::Other(OtherBreakpoint::InsideLockClosure),
            251 => Breakpoint::Exit(ExitBreakpoint::SoftwareTaskEnd),
            252 => Breakpoint::Exit(ExitBreakpoint::ResourceLockEnd),
            253 => Breakpoint::Exit(ExitBreakpoint::HardwareTaskEnd),
            254 => Breakpoint::Other(OtherBreakpoint::InsideLock),
            255 => Breakpoint::Other(OtherBreakpoint::ReplayStart),
            _ => Breakpoint::Other(OtherBreakpoint::Invalid),
        }
    }
}
