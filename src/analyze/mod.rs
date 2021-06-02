//! Stack resource policy based response-time analysis.
//!
mod analysis;
pub(crate) mod data;

use crate::cli::AnalyzeInput;
use crate::measure::Trace;
use analysis::*;
use anyhow::{anyhow, Result};
pub(crate) use data::{Task, TaskResources, Tasks};
use std::{fs::File, io::Read};
use toml;

use self::data::Priorities;

type ResponseTimes = Vec<(String, u32, u32, u32, u32, f32)>;

/// Calculates the response times and runs a schedulability analysis of the results
/// from the measurements of the test vectors on hardware. Runs the analysis on each
/// task's WCET.
pub fn response_time_analysis(input: &AnalyzeInput) -> Result<()> {
    let mut details_file = match &input.details {
        Some(file) => File::open(file)?,
        None => return Err(anyhow!("no file given as input")),
    };
    let mut details_contents = String::new();
    details_file.read_to_string(&mut details_contents)?;

    let mut measurements_file = match &input.measurements {
        Some(file) => File::open(file)?,
        None => return Err(anyhow!("no file given as input")),
    };
    let mut measurements_contents = String::new();
    measurements_file.read_to_string(&mut measurements_contents)?;

    let tasks: Tasks = toml::from_str(&details_contents)?;
    let mut tasks = tasks.tasks;
    let traces: Vec<Trace> = serde_json::from_str(&measurements_contents)?;

    let (t, p, traces) = data::pre_analysis(&tasks, &traces);

    for mut task in &mut tasks {
        for trace in &traces {
            if trace.name == task.name {
                task.trace = Some(trace.clone());
                break;
            }
        }
    }

    let res = run_analysis(&tasks, &p, &t)?;

    println!("Tasks: {:#?}", &tasks);
    println!("Task resources: {:#?}", &t);
    println!("Priorities: {:#?}", &p);
    println!("Results: {:#?}", &res);

    Ok(())
}

// Calculates the response time of a all tasks. R = B + C + I.
// And the load factor.
// Returns a vector with the above values
fn run_analysis(
    tasks: &Vec<Task>,
    priorities: &Priorities,
    tr: &TaskResources,
) -> Result<Vec<AnalysisResult>> {
    let mut res = Vec::new();

    for task in tasks {
        let c = wcet(task);
        let b = blocking_time(task, &tasks, &priorities, &tr);
        let i = preemption(task, &tasks, &priorities, &tr)?;
        let r = c + b + i;
        let l = load_factor(&task);
        res.push(AnalysisResult {
            name: task.name.to_string(),
            response_time: r,
            wcet: c,
            blocking_time: b,
            preemption_time: i,
            load_factor: l,
        });
    }

    Ok(res)
}

#[derive(Debug)]
pub struct AnalysisResult {
    pub name: String,
    pub response_time: u32,
    pub wcet: u32,
    pub blocking_time: u32,
    pub preemption_time: u32,
    pub load_factor: f32,
}
