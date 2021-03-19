pub mod dwarf;

use crate::cli::Analysis;
use crate::utils::{klee::parse_ktest_files, probe as core_utils};
use anyhow::{anyhow, Context, Result};
use dwarf::ObjectLocationMap;
use ktest_parser::KTest;
use probe_rs::{Core, MemoryInterface, Probe};

const HALT_TIMEOUT_SECONDS: u64 = 5;

#[derive(Debug, Clone)]
enum Breakpoint {
    Other(Other),
    Entry(Entry),
    Exit(Exit),
}

#[derive(Debug, Clone)]
enum Entry {
    SoftwareTaskStart = 1,
    HardwareTaskStart = 2,
    ResourceLockStart = 3,
}

#[derive(Debug, Clone)]
enum Exit {
    ResourceLockEnd = 252,
    HardwareTaskEnd = 253,
    SoftwareTaskEnd = 254,
}

#[derive(Debug, Clone)]
enum Other {
    Default = 0,
    Invalid = 100,
    ReplayStart = 255,
}

impl From<u8> for Breakpoint {
    fn from(u: u8) -> Breakpoint {
        match u {
            0 => Breakpoint::Other(Other::Default),
            1 => Breakpoint::Entry(Entry::SoftwareTaskStart),
            2 => Breakpoint::Entry(Entry::HardwareTaskStart),
            3 => Breakpoint::Entry(Entry::ResourceLockStart),
            252 => Breakpoint::Exit(Exit::ResourceLockEnd),
            253 => Breakpoint::Exit(Exit::HardwareTaskEnd),
            254 => Breakpoint::Exit(Exit::SoftwareTaskEnd),
            255 => Breakpoint::Other(Other::ReplayStart),
            _ => Breakpoint::Other(Other::Invalid),
        }
    }
}

#[derive(Debug, Clone)]
pub enum TraceType {
    SoftwareTask,
    HardwareTask,
    ResourceLock,
}

impl From<Entry> for TraceType {
    fn from(e: Entry) -> TraceType {
        match e {
            Entry::SoftwareTaskStart => TraceType::SoftwareTask,
            Entry::HardwareTaskStart => TraceType::HardwareTask,
            Entry::ResourceLockStart => TraceType::ResourceLock,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Trace {
    /// The name of the object.
    pub name: String,
    /// The type of trace of the object.
    pub ttype: TraceType,
    /// Clock cycle when this object is executing.
    pub start: u32,
    /// List of critical sections and blocking objects.
    pub inner: Vec<Trace>,
    /// Clock cycle when this oject has finished executing.
    pub end: u32,
}

impl Trace {
    fn new(name: String, ttype: TraceType, start: u32, inner: Vec<Trace>, end: u32) -> Trace {
        Trace {
            name,
            ttype,
            start,
            inner,
            end,
        }
    }
}

pub fn analyze(a: Analysis) -> Result<()> {
    let ktests = parse_ktest_files(&a.ktests);
    let dwarf = dwarf::get_replay_addresses(&a.dwarf)?;

    println!("{:#x?}", ktests);
    println!("{:#x?}", dwarf);

    let probes = Probe::list_all();
    let probe = probes[0].open()?;

    let mut session = probe.attach(a.chip)?;

    let mut core = session.core(0)?;

    // Analysis
    for ktest in ktests {
        println!("-------------------------------------------------------------");
        // Continue until reaching BKPT 255 (replaystart)
        run_to_replay_start(&mut core).context("Could not continue to replay start")?;
        write_replay_objects(&mut core, &ktest, &dwarf)
            .with_context(|| format!("Could not replay with KTest: {:?}", &ktest))?;

        let mut stack: Vec<Entry> = Vec::new();
        let trace = match wcet_analysis(&mut core, &mut stack) {
            Ok(trace) => trace,
            Err(e) => {
                println!("Error when running WCET analysis: {:?}", e);
                Vec::<Trace>::new()
            }
        };
        println!("{:#?}", trace);
    }

    Ok(())
}

/// Runs to where the replay starts.
fn run_to_replay_start(core: &mut Core) -> Result<()> {
    // Wait for core to halt on a breakpoint. If it doesn't
    // something is wrong.
    core.wait_for_core_halted(std::time::Duration::from_secs(HALT_TIMEOUT_SECONDS))?;
    loop {
        let imm = core_utils::read_breakpoint_value(core)?;
        // Ready to analyze when reaching this breakpoint
        if imm == Other::ReplayStart as u8 {
            break;
        }
        // Should there be other breakpoints we continue past them
        core_utils::run(core)?;
    }
    Ok(())
}

/// Writes the replay contents of the KTEST file to the objects memory addresses.
fn write_replay_objects(
    core: &mut Core,
    ktest: &KTest,
    locations: &ObjectLocationMap,
) -> Result<()> {
    for test in &ktest.objects {
        let location = locations.get(&test.name);
        match location {
            Some(addr) => {
                let a = addr.unwrap() as u32;
                let slice = test.bytes.as_slice();
                // Remove this block later
                {
                    println!("Writing {:?} to address {:x?}", slice, a);
                }
                core.write_8(a, slice)?;
            }
            None => {
                // Should log a warning here instead
                return Err(anyhow!("Address was not found"));
            }
        }
        // Remove this block later
        {
            let loc = location.unwrap().unwrap();
            let value = core.read_word_32(loc as u32)?;
            println!("THE VALUE READ FROM RAM: {:b}, {:x?}", value, value);
        }
    }
    Ok(())
}

fn wcet_analysis(core: &mut Core, stack: &mut Vec<Entry>) -> Result<Vec<Trace>> {
    // 1.
    // Go to next breakpoint
    core_utils::run(core).context("Could not continue from replay start")?;
    // Wait for core to halt on a breakpoint. If it doesn't
    // something is wrong.
    core.wait_for_core_halted(std::time::Duration::from_secs(HALT_TIMEOUT_SECONDS))?;
    if !core_utils::breakpoint_at_pc(core)? {
        return Err(anyhow!(
            "Core halted, but not due to breakpoint. Can't continue with analysis."
        ));
    }

    // 2.
    // Read breakpoint value
    let mut traces: Vec<Trace> = Vec::new();
    let imm = Breakpoint::from(core_utils::read_breakpoint_value(core)?);
    match imm {
        Breakpoint::Entry(e) => {
            stack.push(e.clone());
            let name = "".to_string();
            let ttype = TraceType::from(e.clone());
            let start = core_utils::read_cycle_counter(core)?;
            let inner = wcet_analysis(core, stack).with_context(|| {
                format!("Could not proceed with analysis after breakpoint {:?}", &e)
            })?;
            let end = core_utils::read_cycle_counter(core)?;
            let trace = Trace::new(name, ttype, start, inner, end);
            traces.push(trace);
        }
        Breakpoint::Exit(exit) => {
            // The stack should not be empty if we're exiting the analysis
            let entry = stack.pop().unwrap() as u32;
            let exit = exit as u32;
            if entry + exit != 255 {
                return Err(anyhow!(
                    "Breakpoint scope not matching! Got entry: {} and exit: {}",
                    entry,
                    exit
                ));
            }
        }
        //Breakpoint::Other(Other::Default) => (),
        Breakpoint::Other(o) => {
            return Err(anyhow!("Unsupported breakpoint inside analysis: {:?}", o));
        }
    }
    Ok(traces)
}
