pub mod dwarf;

use crate::cli::Analysis;
use crate::utils::{klee::parse_ktest_files, probe as core_utils};
use anyhow::{anyhow, Context, Result};
use dwarf::ObjectLocationMap;
use ktest_parser::KTest;
use probe_rs::{Core, MemoryInterface, Probe};

const HALT_TIMEOUT_SECONDS: u64 = 5;

#[derive(Debug, Clone, PartialEq)]
enum Breakpoint {
    Other(Other),
    Entry(Entry),
    Exit(Exit),
}

impl Breakpoint {
    fn is_entry(&self) -> bool {
        match self {
            Breakpoint::Entry(_) => true,
            _ => false,
        }
    }

    fn is_exit(&self) -> bool {
        match self {
            Breakpoint::Exit(_) => true,
            _ => false,
        }
    }

    fn is_other(&self) -> bool {
        match self {
            Breakpoint::Other(_) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Entry {
    SoftwareTaskStart = 1,
    HardwareTaskStart = 2,
    ResourceLockStart = 3,
}

#[derive(Debug, Clone, PartialEq)]
enum Exit {
    ResourceLockEnd = 252,
    HardwareTaskEnd = 253,
    SoftwareTaskEnd = 254,
}

#[derive(Debug, Clone, PartialEq)]
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
    let trace: Vec<(Breakpoint, String, u32)> = vec![
        (
            Breakpoint::Entry(Entry::HardwareTaskStart),
            String::from("task1"),
            0,
        ),
        (
            Breakpoint::Entry(Entry::ResourceLockStart),
            String::from("res1"),
            5,
        ),
        (
            Breakpoint::Entry(Entry::ResourceLockStart),
            String::from("res2"),
            10,
        ),
        (
            Breakpoint::Exit(Exit::ResourceLockEnd),
            String::from("res2"),
            15,
        ),
        (
            Breakpoint::Exit(Exit::ResourceLockEnd),
            String::from("res1"),
            15,
        ),
        (
            Breakpoint::Entry(Entry::ResourceLockStart),
            String::from("res3"),
            15,
        ),
        (
            Breakpoint::Exit(Exit::ResourceLockEnd),
            String::from("res3"),
            20,
        ),
        (
            Breakpoint::Exit(Exit::HardwareTaskEnd),
            String::from("task1"),
            20,
        ),
    ];

    println!("TRACE: {:#?}", &trace);
    let res = wcet_analysis(&trace[..])?;
    println!("ANALYSIS RESULTS: {:#?}", res);

    Ok(())
}

pub fn _analyze(a: Analysis) -> Result<()> {
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
        let bkpts = read_breakpoints(&mut core)?;
        println!("{:#?}", bkpts);
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

/// Read all breakpoints and the cycle counter at their positions
/// between the ReplayStart breakpoints and return them as a list
fn read_breakpoints(core: &mut Core) -> Result<Vec<(Breakpoint, String, u32)>> {
    let mut stack: Vec<(Breakpoint, String, u32)> = Vec::new();

    loop {
        core_utils::run(core).context("Could not continue from replay start")?;
        core.wait_for_core_halted(std::time::Duration::from_secs(HALT_TIMEOUT_SECONDS))?;
        if !core_utils::breakpoint_at_pc(core)? {
            return Err(anyhow!(
                "Core halted, but not due to breakpoint. Can't continue with analysis."
            ));
        }

        // Read breakpoint immediate value
        let imm = Breakpoint::from(core_utils::read_breakpoint_value(core)?);
        if imm == Breakpoint::Other(Other::ReplayStart) {
            break;
        }

        let cyccnt = core_utils::read_cycle_counter(core)?;
        stack.push((imm, "".to_string(), cyccnt));
    }

    Ok(stack)
}

fn wcet_analysis(bkpts: &[(Breakpoint, String, u32)]) -> Result<Vec<Trace>> {
    let mut temp: Vec<Entry> = Vec::new();
    let mut bkpt_stack = bkpts.to_vec();
    bkpt_stack.reverse();
    let (traces, _) = wcet_rec(&mut bkpt_stack, &mut temp)?;
    Ok(traces)
}

fn wcet_rec(
    bkpts: &mut Vec<(Breakpoint, String, u32)>,
    stack: &mut Vec<Entry>,
) -> Result<(Vec<Trace>, (Breakpoint, String, u32))> {
    let mut traces: Vec<Trace> = Vec::new();
    let (bkpt, name, cyccnt) = bkpts.pop().unwrap();
    let mut curr_bkpt = bkpt.clone();
    let mut curr_name = name.clone();
    let mut curr_cyccnt = cyccnt.clone();

    println!("@ BKPTS: {:#?}", bkpts);

    match &curr_bkpt.clone() {
        Breakpoint::Entry(e) => {
            println!("* ENTRY: {:?}. Name: {:?}", &e, &curr_name);
            stack.push(e.clone());
            let name = curr_name.clone();
            let ttype = TraceType::from(e.clone());
            let start = curr_cyccnt.clone();
            let mut inner = Vec::<Trace>::new();

            // Inner loop
            let mut prev: Breakpoint = curr_bkpt.clone();
            let mut end;
            loop {
                let (mut i, (last, n, e)) = wcet_rec(bkpts, stack).with_context(|| {
                    format!("Could not proceed with analysis after breakpoint {:?}", &e)
                })?;
                inner.append(&mut i);
                prev = last.clone();
                end = e;

                // If we get two Exits in a row, the loop should break
                if last.is_exit() && prev.is_exit() || bkpts.is_empty() {
                    println!(
                        "** Inner. BREAK. Last: {:?}. Prev: {:?}. Name: {:?}",
                        &last, &prev, &name
                    );
                    break;
                } else {
                    println!("** Inner. CONTINUE. Last: {:?}", &last);
                }
            }
            let trace = Trace::new(name, ttype, start, inner, end);
            traces.push(trace);
        }
        Breakpoint::Exit(exit) => {
            println!("* EXIT: {:?}. Name: {:?}", &exit, &curr_name);
            // The stack should not be empty if we're exiting the analysis
            let entry = stack.pop().unwrap() as u32;
            let exit = exit.clone() as u32;
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
            println!("* OTHER: {:?}", &o);
            return Err(anyhow!("Unsupported breakpoint inside analysis: {:?}", o));
        }
    }

    Ok((traces, (curr_bkpt, curr_name, curr_cyccnt)))
}

fn _wcet_analysis_rec(core: &mut Core, stack: &mut Vec<Entry>) -> Result<(Vec<Trace>, Breakpoint)> {
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
    let mut current = imm.clone();
    match imm.clone() {
        Breakpoint::Entry(e) => {
            println!("* ENTRY: {:?}", &e);
            stack.push(e.clone());
            let name = "".to_string();
            let ttype = TraceType::from(e.clone());
            let start = core_utils::read_cycle_counter(core)?;
            let mut inner = Vec::<Trace>::new();
            // Inner loop
            let mut prev: Breakpoint = imm.clone();
            loop {
                let (mut i, last) = _wcet_analysis_rec(core, stack).with_context(|| {
                    format!("Could not proceed with analysis after breakpoint {:?}", &e)
                })?;
                inner.append(&mut i);

                // If we get two Exits in a row, the loop should break
                // Or if i is empty it means the scope should end.
                if (last.is_exit() && prev.is_exit()) || i.is_empty() {
                    println!("** Inner. BREAK. Last: {:?}", &last);
                    current = last.clone();
                    break;
                } else {
                    println!("** Inner. CONTINUE. Last: {:?}", &last);
                    current = last.clone();
                    prev = last;
                }
            }
            let end = core_utils::read_cycle_counter(core)?;
            let trace = Trace::new(name, ttype, start, inner, end);
            traces.push(trace);
        }
        Breakpoint::Exit(exit) => {
            println!("* EXIT: {:?}", &exit);
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
            println!("* OTHER: {:?}", &o);
            return Err(anyhow!("Unsupported breakpoint inside analysis: {:?}", o));
        }
    }
    Ok((traces, current))
}
