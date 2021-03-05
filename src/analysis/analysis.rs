use crate::utils;

use probe_rs::{MemoryInterface, Session};
use std::sync::Mutex;

const CYCCNT: u32 = 0xe000_1004;

pub fn _analysis(session: &Mutex<Session>) {
    let mut buff = [0u32; 1];

    let mut session = session.lock().unwrap();
    let mut core = session.core(0).unwrap();

    core.run().unwrap();
    core.wait_for_core_halted(std::time::Duration::from_secs(5))
        .unwrap();

    core.read_32(CYCCNT, &mut buff).unwrap();
    println!("cyccnt {:?}", buff);
    utils::run_from_breakpoint(&mut core);
    core.wait_for_core_halted(std::time::Duration::from_secs(5))
        .unwrap();

    core.read_32(CYCCNT, &mut buff).unwrap();
    println!("cyccnt {:?}", buff);
    utils::run_from_breakpoint(&mut core);
    core.wait_for_core_halted(std::time::Duration::from_secs(5))
        .unwrap();

    core.read_32(CYCCNT, &mut buff).unwrap();
    println!("cyccnt {:?}", buff);
    utils::run_from_breakpoint(&mut core);
    core.wait_for_core_halted(std::time::Duration::from_secs(5))
        .unwrap();
}
