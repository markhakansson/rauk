//! Stack resource policy based response-time analysis.
//!
mod data;

use crate::cli::AnalyzeInput;
use crate::measure::Trace;
use anyhow::{anyhow, Result};
use data::Tasks;
use std::{fs::File, io::Read};
use toml;

pub fn response_times(input: &AnalyzeInput) -> Result<()> {
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
    let traces: Vec<Trace> = serde_json::from_str(&measurements_contents)?;

    let (t, p) = data::pre_analysis(&tasks.tasks, &traces);
    println!("Task resources: {:#?}", &t);
    println!("Priorities: {:#?}", &p);

    Ok(())
}
