use crate::measure::Trace;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// A map of the tasks and resources with their priorities
pub type Priorities = HashMap<String, u8>;
/// A map of tasks and the resources they are accessing
pub type TaskResources = HashMap<String, HashSet<String>>;

pub type TaskMap = HashMap<String, Task>;

#[derive(Debug, Serialize, Deserialize)]
pub struct Tasks {
    pub tasks: Vec<Task>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// The name of the RTIC task (same as the function name)
    pub name: String,
    /// The priority of the RTIC task (same as the given priority)
    pub priority: u8,
    /// The expected deadline (in clock cycles)
    pub deadline: u32,
    /// The expected inter-arrival time (in clock cycles)
    pub inter_arrival: u32,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resource {
    /// The name of the RTIC resource
    pub name: String,
    /// The priority ceiling of the RTIC resource
    pub priority: u8,
}

pub fn pre_analysis(tasks: &Vec<Task>, traces: &Vec<Trace>) -> (TaskResources, Priorities) {
    let mut task_map: TaskMap = HashMap::new();
    for task in tasks {
        task_map.insert(task.name.clone(), task.clone());
    }

    let mut task_resources = get_task_resources(traces, &task_map);
    let priorities = get_priorites(&tasks, &mut task_resources);

    (task_resources, priorities)
}

/// Returns a map of tasks and the resources they access
fn get_task_resources(traces: &Vec<Trace>, tasks: &TaskMap) -> TaskResources {
    let mut task_resources: TaskResources = HashMap::new();

    for trace in traces {
        if let Some(task) = tasks.get(&trace.name) {
            update_task_resources(&task, &trace.inner, &mut task_resources);
        }
    }

    task_resources
}

fn update_task_resources(task: &Task, traces: &Vec<Trace>, task_resources: &mut TaskResources) {
    for trace in traces {
        if let Some(set) = task_resources.get_mut(&task.name) {
            set.insert(trace.name.clone());
        }
        update_task_resources(task, &trace.inner, task_resources);
    }
}

/// Returns a map of tasks and resources with their respective priorities/ceilings
fn get_priorites(tasks: &Vec<Task>, task_resources: &mut TaskResources) -> Priorities {
    let mut priorities: Priorities = HashMap::new();

    for task in tasks {
        priorities.insert(task.name.clone(), task.priority);

        if let Some(set) = task_resources.get(&task.name) {
            for resource in set.iter() {
                if let Some(priority) = priorities.get(resource) {
                    if &task.priority > priority {
                        priorities.insert(resource.clone(), task.priority);
                    }
                }
            }
        }
    }

    priorities
}
