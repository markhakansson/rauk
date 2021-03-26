use anyhow::{anyhow, Context, Result};

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

#[derive(Debug, Clone, PartialEq)]
pub enum Breakpoint {
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
pub enum Entry {
    HardwareTaskStart = 2,
    ResourceLockStart = 3,
    SoftwareTaskStart = 4,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Exit {
    SoftwareTaskEnd = 251,
    ResourceLockEnd = 252,
    HardwareTaskEnd = 253,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Other {
    Default = 0,
    InsideTask = 1,
    Invalid = 100,
    InsideLock = 254,
    ReplayStart = 255,
}

impl From<u8> for Breakpoint {
    fn from(u: u8) -> Breakpoint {
        match u {
            0 => Breakpoint::Other(Other::Default),
            1 => Breakpoint::Other(Other::InsideTask),
            2 => Breakpoint::Entry(Entry::HardwareTaskStart),
            3 => Breakpoint::Entry(Entry::ResourceLockStart),
            4 => Breakpoint::Entry(Entry::SoftwareTaskStart),
            251 => Breakpoint::Exit(Exit::SoftwareTaskEnd),
            252 => Breakpoint::Exit(Exit::ResourceLockEnd),
            253 => Breakpoint::Exit(Exit::HardwareTaskEnd),
            254 => Breakpoint::Other(Other::InsideLock),
            255 => Breakpoint::Other(Other::ReplayStart),
            _ => Breakpoint::Other(Other::Invalid),
        }
    }
}

pub fn wcet_analysis(mut bkpts: Vec<(Breakpoint, String, u32)>) -> Result<Vec<Trace>> {
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
    fn test_analysis_multiple_locks() {
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
    fn test_analysis_nested_locks() {
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
                Breakpoint::Entry(Entry::ResourceLockStart),
                String::from("res2"),
                15,
            ),
            (
                Breakpoint::Entry(Entry::ResourceLockStart),
                String::from("res3"),
                25,
            ),
            (
                Breakpoint::Exit(Exit::ResourceLockEnd),
                String::from("res3"),
                35,
            ),
            (
                Breakpoint::Exit(Exit::ResourceLockEnd),
                String::from("res2"),
                45,
            ),
            (
                Breakpoint::Exit(Exit::ResourceLockEnd),
                String::from("res1"),
                55,
            ),
            (
                Breakpoint::Exit(Exit::SoftwareTaskEnd),
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
