use super::data;
use super::data::Priorities;
use super::data::Task;
use super::TaskResources;
use crate::measure::Trace;
use anyhow::{anyhow, Result};
use std::collections::HashSet;

// Calculate load factor of a task
pub fn load_factor(task: &Task) -> f32 {
    let wcet = wcet(task) as f32;
    let inter_arrival = task.inter_arrival as f32;

    wcet / inter_arrival
}

// Calculates the preemption (I(t)) of a task with or without approximation.
pub fn preemption(task: &Task, tasks: &[Task], ip: &Priorities, tr: &TaskResources) -> Result<u32> {
    let base = wcet(task) + blocking_time(task, tasks, ip, tr);
    let premp = preemption_rec(task, tasks, ip, tr, base)?;
    Ok(premp - base)
}

pub fn wcet(task: &Task) -> u32 {
    let t = task.trace.as_ref().unwrap();
    t.end - t.start
}

// Response time analysis recurrence relation
fn preemption_rec(
    task: &Task,
    tasks: &[Task],
    ip: &Priorities,
    tr: &TaskResources,
    prev_rt: u32,
) -> Result<u32> {
    let mut current_rt = wcet(task) + blocking_time(task, tasks, ip, tr);
    let mut task_prio = 0;

    if let Some(prio) = ip.get(&task.name) {
        task_prio = *prio
    }

    // The summation part of eq. 7.22 in Hard Real-Time Computing Systems
    for t in tasks {
        if let Some(t_prio) = ip.get(&t.name) {
            if t_prio > &task_prio {
                let a = t.inter_arrival as f32;
                let calc = wcet(t) * (prev_rt as f32 / a).ceil() as u32;
                current_rt += calc;
            }
        }
    }

    if current_rt > task.deadline {
        return Err(anyhow!("Response time is larger than the deadline!"));
    }

    if current_rt == prev_rt {
        Ok(current_rt)
    } else {
        preemption_rec(task, tasks, ip, tr, current_rt)
    }
}

// Blocking function. Calculates the largest amount of time a task (T1) may be blocked by
// another task (T2) using a resource (R). P(T2) < P(T1) and P(R) >= P(T1).
pub fn blocking_time(task: &Task, tasks: &[Task], ip: &Priorities, tr: &TaskResources) -> u32 {
    let mut max_block_time = 0;
    let mut task_prio = 0;
    let mut resources = &HashSet::new();

    // Get the task's priority
    if let Some(prio) = ip.get(&task.name) {
        task_prio = *prio
    }
    // Get the resources used by the task
    if let Some(set) = tr.get(&task.name) {
        resources = set
    }

    for r in resources.iter() {
        for t in tasks {
            if let (Some(t_prio), Some(r_ceil)) = (ip.get(&t.name), ip.get(r)) {
                // Compare the priority and ceiling with the task prio
                if t_prio < &task_prio && r_ceil >= &task_prio {
                    let time = max_time_hold_resource(&t.trace.as_ref().unwrap(), r);
                    if time > max_block_time {
                        max_block_time = time;
                    }
                }
            }
        }
    }

    max_block_time
}

// Get the maximum length of time for which the resource is hold in a trace
fn max_time_hold_resource(trace: &Trace, res_name: &str) -> u32 {
    let mut max_time = 0;

    if trace.name == res_name {
        max_time = trace.end - trace.start;
    }

    // Recursively calculate the max time
    if !trace.inner.is_empty() {
        for t in &trace.inner {
            let time = max_time_hold_resource(&t, res_name);
            if time > max_time {
                max_time = time;
            }
        }
    }

    max_time
}
