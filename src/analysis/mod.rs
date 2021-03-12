pub mod dwarf;

use crate::cli::Analysis;
use crate::utils::{klee::parse_ktest_files, probe as core_utils};
use anyhow::Result;
use dwarf::ObjectLocationMap;
use ktest_parser::KTest;
use probe_rs::{Core, MemoryInterface, Probe};

pub fn analyze(a: Analysis) -> Result<()> {
    let ktests = parse_ktest_files(&a.ktests.unwrap());
    let dwarf = dwarf::get_replay_addresses(a.dwarf.unwrap())?;

    println!("{:#x?}", ktests);
    println!("{:#x?}", dwarf);

    let probes = Probe::list_all();
    let probe = probes[0].open()?;

    let mut session = probe.attach(a.chip)?;

    let mut core = session.core(0)?;

    run_to_replay_start(&mut core)?;

    for ktest in ktests {
        write_replay_objects(&mut core, &ktest, &dwarf)?;
    }

    Ok(())
}

/// Runs to where the replay starts.
fn run_to_replay_start(core: &mut Core) -> Result<()> {
    // Wait for core to halt on a breakpoint. If it doesn't
    // something is wrong.
    core.wait_for_core_halted(std::time::Duration::from_secs(5))?;
    loop {
        let value = core_utils::read_breakpoint_value(core)?;
        match value {
            Some(imm) => {
                // 255 denotes replay start
                if imm == 255 {
                    println!("{:#?}", value.unwrap());
                    break;
                }
            }
            _ => (),
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
                println!("Writing {:?} to address {:x?}", slice, a);
                //core.write_8(a, slice)?;
            }
            None => {
                // Should log a warning here instead
                panic!("Address was not found!")
            }
        }
    }
    Ok(())
}
