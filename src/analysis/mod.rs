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

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
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
        let bkpts = read_breakpoints(&mut core)?;
        println!("{:#?}", bkpts);
        let trace = wcet_analysis(bkpts);
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
                core.write_8(a, slice)?;
            }
            None => {
                // Should log a warning here instead
                return Err(anyhow!("Address was not found"));
            }
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

fn wcet_analysis(mut bkpts: Vec<(Breakpoint, String, u32)>) -> Result<Vec<Trace>> {
    let mut temp: Vec<Entry> = Vec::new();
    bkpts.reverse();
    let (traces, _) = wcet_rec(&mut bkpts, &mut temp)?;
    Ok(traces)
}

// This function is not the most beautiful code ever written and quite unintuitive!
// Check the documenation for the analysis to get an understanding of how it works!
//
// The `bkpts` contains the tuple (Breakpoint, Name, CYCCNT) of each breakpoint, traced
// from the replay harness on actual hardware. The `stack` is used internally to keep
// track of the correct scopes. That is, that for each Entry a corresponding Exit exists.
fn wcet_rec(
    bkpts: &mut Vec<(Breakpoint, String, u32)>,
    stack: &mut Vec<Entry>,
) -> Result<(Vec<Trace>, (Breakpoint, String, u32))> {
    // This is the main result of this function
    let mut traces: Vec<Trace> = Vec::new();
    let (bkpt, name, cyccnt) = match bkpts.pop() {
        Some((b, n, c)) => (b, n, c),
        None => return Err(anyhow!("Breakpoint vector is empty")),
    };

    // Set the current scope's variables. These are always returned in the end.
    // Because the outer scope needs to be able to read the objects data.
    let curr_bkpt = bkpt.clone();
    let curr_name = name.clone();
    let curr_cyccnt = cyccnt.clone();

    match &curr_bkpt {
        Breakpoint::Entry(e) => {
            // Push this entry to the internal stack. Used to check
            // that the corresponding Entry, Exit are correct.
            stack.push(e.clone());

            // Build a new trace
            let name = curr_name.clone();
            let ttype = TraceType::from(e.clone());
            let start = curr_cyccnt.clone();
            let mut inner = Vec::<Trace>::new();

            // Inner loop
            let mut prev: Breakpoint;
            let mut end;
            loop {
                let (mut i, (last, _, e)) = wcet_rec(bkpts, stack).with_context(|| {
                    format!("Could not proceed with analysis after breakpoint {:?}", &e)
                })?;
                inner.append(&mut i);
                prev = last.clone();
                end = e;

                // If we get two Exits in a row, it means that we're exiting
                // the inner loop. It should also break if there are no more
                // objects in the bkpts vector
                if last.is_exit() && prev.is_exit() || bkpts.is_empty() {
                    break;
                }
            }
            let trace = Trace::new(name, ttype, start, inner, end);
            traces.push(trace);
        }
        Breakpoint::Exit(exit) => {
            // The stack should not be empty if we're exiting the analysis.
            // All corresponding Entry/Exit should add up to 255 if correct order.
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
        // Should ignore the Default breakpoint instead of returning an error
        Breakpoint::Other(o) => {
            return Err(anyhow!("Unsupported breakpoint inside analysis: {:?}", o));
        }
    }

    Ok((traces, (curr_bkpt, curr_name, curr_cyccnt)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_analysis_nested_and_multiple_locks() {
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

        let analysis = wcet_analysis(trace).unwrap();
        let result = analysis.first().unwrap();
        let expected = Trace {
            name: "task1".to_string(),
            ttype: TraceType::HardwareTask,
            start: 0,
            inner: vec![
                Trace {
                    name: "res1".to_string(),
                    ttype: TraceType::ResourceLock,
                    start: 5,
                    inner: vec![Trace {
                        name: "res2".to_string(),
                        ttype: TraceType::ResourceLock,
                        start: 10,
                        inner: vec![],
                        end: 15,
                    }],
                    end: 15,
                },
                Trace {
                    name: "res3".to_string(),
                    ttype: TraceType::ResourceLock,
                    start: 15,
                    inner: vec![],
                    end: 20,
                },
            ],
            end: 20,
        };
        assert_eq!(result, &expected)
    }

    #[test]
    fn test_multiple_locks() {
        let trace: Vec<(Breakpoint, String, u32)> = vec![
            (
                Breakpoint::Entry(Entry::SoftwareTaskStart),
                String::from("task1"),
                0,
            ),
            (
                Breakpoint::Entry(Entry::ResourceLockStart),
                String::from("res1"),
                5,
            ),
            (
                Breakpoint::Exit(Exit::ResourceLockEnd),
                String::from("res1"),
                15,
            ),
            (
                Breakpoint::Entry(Entry::ResourceLockStart),
                String::from("res2"),
                15,
            ),
            (
                Breakpoint::Exit(Exit::ResourceLockEnd),
                String::from("res2"),
                20,
            ),
            (
                Breakpoint::Entry(Entry::ResourceLockStart),
                String::from("res3"),
                20,
            ),
            (
                Breakpoint::Exit(Exit::ResourceLockEnd),
                String::from("res3"),
                25,
            ),
            (
                Breakpoint::Exit(Exit::SoftwareTaskEnd),
                String::from("task1"),
                30,
            ),
        ];
        let analysis = wcet_analysis(trace).unwrap();
        let result = analysis.first().unwrap();
        let expected = Trace {
            name: "task1".to_string(),
            ttype: TraceType::SoftwareTask,
            start: 0,
            inner: vec![
                Trace {
                    name: "res1".to_string(),
                    ttype: TraceType::ResourceLock,
                    start: 5,
                    inner: vec![],
                    end: 15,
                },
                Trace {
                    name: "res2".to_string(),
                    ttype: TraceType::ResourceLock,
                    start: 15,
                    inner: vec![],
                    end: 20,
                },
                Trace {
                    name: "res3".to_string(),
                    ttype: TraceType::ResourceLock,
                    start: 20,
                    inner: vec![],
                    end: 25,
                },
            ],
            end: 30,
        };
        assert_eq!(result, &expected);
    }

    #[test]
    fn test_analysis_invalid_input_size() {
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
                Breakpoint::Exit(Exit::HardwareTaskEnd),
                String::from("task1"),
                10,
            ),
        ];
        let analysis = wcet_analysis(trace);
        assert!(analysis.is_err());
    }

    #[test]
    fn test_analysis_empty_input() {
        let trace: Vec<(Breakpoint, String, u32)> = vec![];
        let analysis = wcet_analysis(trace);
        assert!(analysis.is_err());
    }

    #[test]
    fn test_analysis_empty_inner_trace() {
        let trace: Vec<(Breakpoint, String, u32)> = vec![
            (
                Breakpoint::Entry(Entry::HardwareTaskStart),
                String::from("task1"),
                0,
            ),
            (
                Breakpoint::Exit(Exit::HardwareTaskEnd),
                String::from("task1"),
                10,
            ),
        ];
        let analysis = wcet_analysis(trace).unwrap();
        let result = analysis.first().unwrap();
        let expected = Trace {
            name: "task1".to_string(),
            ttype: TraceType::HardwareTask,
            start: 0,
            inner: vec![],
            end: 10,
        };
        assert_eq!(result, &expected);
    }

    #[test]
    fn test_analysis_wrong_task_order() {
        let trace: Vec<(Breakpoint, String, u32)> = vec![
            (
                Breakpoint::Entry(Entry::HardwareTaskStart),
                String::from("task1"),
                0,
            ),
            (
                Breakpoint::Exit(Exit::SoftwareTaskEnd),
                String::from("task1"),
                10,
            ),
        ];
        let analysis = wcet_analysis(trace);
        assert!(analysis.is_err());
    }

    #[test]
    fn test_analysis_wrong_lock_order() {
        let trace: Vec<(Breakpoint, String, u32)> = vec![
            (
                Breakpoint::Entry(Entry::ResourceLockStart),
                String::from("res1"),
                0,
            ),
            (
                Breakpoint::Exit(Exit::SoftwareTaskEnd),
                String::from("task1"),
                10,
            ),
        ];
        let analysis = wcet_analysis(trace);
        assert!(analysis.is_err());
    }
}
