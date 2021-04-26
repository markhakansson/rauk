use super::{
    breakpoints::{Breakpoint, EntryBreakpoint},
    measurement::MeasurementResult,
};
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

/// The different types a Trace can be
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TraceType {
    SoftwareTask,
    HardwareTask,
    ResourceLock,
}

impl From<EntryBreakpoint> for TraceType {
    fn from(e: EntryBreakpoint) -> TraceType {
        match e {
            EntryBreakpoint::SoftwareTaskStart => TraceType::SoftwareTask,
            EntryBreakpoint::HardwareTaskStart => TraceType::HardwareTask,
            EntryBreakpoint::ResourceLockStart => TraceType::ResourceLock,
        }
    }
}

/// The RAUK analysis trace. Contains information about the test replays.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

/// Run a WCET analysis on the given measurements and return a list of traces.
///
/// * `measurements` - A list of MeasurementResults measured on hardware
pub fn wcet_analysis(mut measurements: Vec<MeasurementResult>) -> Result<Vec<Trace>> {
    let mut temp: Vec<EntryBreakpoint> = Vec::new();
    measurements.reverse();
    let (traces, _) = wcet_rec(&mut measurements, &mut temp)?;
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
    stack: &mut Vec<EntryBreakpoint>,
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
    use super::super::breakpoints::ExitBreakpoint;
    use super::*;
    #[test]
    fn test_analysis_nested_and_multiple_locks() {
        let trace: Vec<(Breakpoint, String, u32)> = vec![
            (
                Breakpoint::Entry(EntryBreakpoint::HardwareTaskStart),
                String::from("task1"),
                0,
            ),
            (
                Breakpoint::Entry(EntryBreakpoint::ResourceLockStart),
                String::from("res1"),
                5,
            ),
            (
                Breakpoint::Entry(EntryBreakpoint::ResourceLockStart),
                String::from("res2"),
                10,
            ),
            (
                Breakpoint::Exit(ExitBreakpoint::ResourceLockEnd),
                String::from("res2"),
                15,
            ),
            (
                Breakpoint::Exit(ExitBreakpoint::ResourceLockEnd),
                String::from("res1"),
                15,
            ),
            (
                Breakpoint::Entry(EntryBreakpoint::ResourceLockStart),
                String::from("res3"),
                15,
            ),
            (
                Breakpoint::Exit(ExitBreakpoint::ResourceLockEnd),
                String::from("res3"),
                20,
            ),
            (
                Breakpoint::Exit(ExitBreakpoint::HardwareTaskEnd),
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
    fn test_analysis_multiple_locks() {
        let trace: Vec<(Breakpoint, String, u32)> = vec![
            (
                Breakpoint::Entry(EntryBreakpoint::SoftwareTaskStart),
                String::from("task1"),
                0,
            ),
            (
                Breakpoint::Entry(EntryBreakpoint::ResourceLockStart),
                String::from("res1"),
                5,
            ),
            (
                Breakpoint::Exit(ExitBreakpoint::ResourceLockEnd),
                String::from("res1"),
                15,
            ),
            (
                Breakpoint::Entry(EntryBreakpoint::ResourceLockStart),
                String::from("res2"),
                15,
            ),
            (
                Breakpoint::Exit(ExitBreakpoint::ResourceLockEnd),
                String::from("res2"),
                20,
            ),
            (
                Breakpoint::Entry(EntryBreakpoint::ResourceLockStart),
                String::from("res3"),
                20,
            ),
            (
                Breakpoint::Exit(ExitBreakpoint::ResourceLockEnd),
                String::from("res3"),
                25,
            ),
            (
                Breakpoint::Exit(ExitBreakpoint::SoftwareTaskEnd),
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
    fn test_analysis_nested_locks() {
        let trace: Vec<(Breakpoint, String, u32)> = vec![
            (
                Breakpoint::Entry(EntryBreakpoint::SoftwareTaskStart),
                String::from("task1"),
                0,
            ),
            (
                Breakpoint::Entry(EntryBreakpoint::ResourceLockStart),
                String::from("res1"),
                5,
            ),
            (
                Breakpoint::Entry(EntryBreakpoint::ResourceLockStart),
                String::from("res2"),
                15,
            ),
            (
                Breakpoint::Entry(EntryBreakpoint::ResourceLockStart),
                String::from("res3"),
                25,
            ),
            (
                Breakpoint::Exit(ExitBreakpoint::ResourceLockEnd),
                String::from("res3"),
                35,
            ),
            (
                Breakpoint::Exit(ExitBreakpoint::ResourceLockEnd),
                String::from("res2"),
                45,
            ),
            (
                Breakpoint::Exit(ExitBreakpoint::ResourceLockEnd),
                String::from("res1"),
                55,
            ),
            (
                Breakpoint::Exit(ExitBreakpoint::SoftwareTaskEnd),
                String::from("task1"),
                60,
            ),
        ];
        let analysis = wcet_analysis(trace).unwrap();
        let result = analysis.first().unwrap();
        let expected = Trace {
            name: "task1".to_string(),
            ttype: TraceType::SoftwareTask,
            start: 0,
            inner: vec![Trace {
                name: "res1".to_string(),
                ttype: TraceType::ResourceLock,
                start: 5,
                inner: vec![Trace {
                    name: "res2".to_string(),
                    ttype: TraceType::ResourceLock,
                    start: 15,
                    inner: vec![Trace {
                        name: "res3".to_string(),
                        ttype: TraceType::ResourceLock,
                        start: 25,
                        inner: vec![],
                        end: 35,
                    }],
                    end: 45,
                }],
                end: 55,
            }],
            end: 60,
        };
        assert_eq!(result, &expected);
    }
    #[test]
    fn test_analysis_invalid_input_size() {
        let trace: Vec<(Breakpoint, String, u32)> = vec![
            (
                Breakpoint::Entry(EntryBreakpoint::HardwareTaskStart),
                String::from("task1"),
                0,
            ),
            (
                Breakpoint::Entry(EntryBreakpoint::ResourceLockStart),
                String::from("res1"),
                5,
            ),
            (
                Breakpoint::Exit(ExitBreakpoint::HardwareTaskEnd),
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
                Breakpoint::Entry(EntryBreakpoint::HardwareTaskStart),
                String::from("task1"),
                0,
            ),
            (
                Breakpoint::Exit(ExitBreakpoint::HardwareTaskEnd),
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
                Breakpoint::Entry(EntryBreakpoint::HardwareTaskStart),
                String::from("task1"),
                0,
            ),
            (
                Breakpoint::Exit(ExitBreakpoint::SoftwareTaskEnd),
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
                Breakpoint::Entry(EntryBreakpoint::ResourceLockStart),
                String::from("res1"),
                0,
            ),
            (
                Breakpoint::Exit(ExitBreakpoint::SoftwareTaskEnd),
                String::from("task1"),
                10,
            ),
        ];
        let analysis = wcet_analysis(trace);
        assert!(analysis.is_err());
    }
}
