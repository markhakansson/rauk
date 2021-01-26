mod cli;

use probe_rs::{
    flashing::{download_file, Format},
    Core, MemoryInterface, Probe, Session,
};
use probe_rs_rtt::Rtt;
use std::str;
use std::sync::{Arc, Mutex};

const CYCCNT: u32 = 0xe000_1004;

fn probe_wcet(opts: cli::CliOptions) {
    // Get a list of all available debug probes.
    let probes = Probe::list_all();
    println!("{:?}", probes);

    // Use the first probe found.
    let probe = probes[0].open().unwrap();

    // Attach to a chip.
    let mut session = probe.attach("STM32F401RETx").unwrap();

    // Flash the card with binary
    download_file(&mut session, opts.path.as_path(), Format::Elf)
        .expect("Could not flash the card");
    println!("Card flashed");

    // Reset the core and halt
    {
        let mut core = session.core(0).unwrap();
        core.reset_and_halt(std::time::Duration::from_secs(1))
            .unwrap();
    }

    println!("Create mutex");
    let session = Arc::new(Mutex::new(session));

    let mut gdb_thread_handle = None;
    if opts.gdb {
        println!("Starting gdb server");
        let session = session.clone();
        gdb_thread_handle = Some(std::thread::spawn(move || {
            let gdb_connection_string = "127.0.0.1:1337";
            if let Err(e) = probe_rs_gdb_server::run(Some(gdb_connection_string), &session) {
                println!("{:?}", e);
            };
        }));
    }

    if opts.wcet {
        analysis(&session);
    } else {
        println!("Starting program");
        let mut session = session.lock().unwrap();
        let mut core = session.core(0).unwrap();

        if !opts.gdb {
            core.run().unwrap();
        }
    }

    if let Some(gdb_thread_handle) = gdb_thread_handle {
        let _ = gdb_thread_handle.join();
    }

    if opts.rtt {
        println!("Attaching to rtt");
        let rtt = Rtt::attach(session.clone()).unwrap();
        rtt_print(rtt);
    }
}

fn analysis(session: &Mutex<Session>) {
    let mut buff = [0u32; 1];

    let mut session = session.lock().unwrap();
    let mut core = session.core(0).unwrap();

    core.run().unwrap();
    core.wait_for_core_halted(std::time::Duration::from_secs(5))
        .unwrap();

    core.read_32(CYCCNT, &mut buff).unwrap();
    println!("cyccnt {:?}", buff);
    run_from_breakpoint(&mut core);
    core.wait_for_core_halted(std::time::Duration::from_secs(5))
        .unwrap();

    core.read_32(CYCCNT, &mut buff).unwrap();
    println!("cyccnt {:?}", buff);
    run_from_breakpoint(&mut core);
    core.wait_for_core_halted(std::time::Duration::from_secs(5))
        .unwrap();

    core.read_32(CYCCNT, &mut buff).unwrap();
    println!("cyccnt {:?}", buff);
    run_from_breakpoint(&mut core);
    core.wait_for_core_halted(std::time::Duration::from_secs(5))
        .unwrap();
}

fn step_from_breakpoint(core: &mut Core) {
    let mut smbf = [0u8; 2];
    let pc = core.registers().program_counter();
    let pc_val = core.read_core_reg(pc).unwrap();
    let step_pc = pc_val + 0x2;
    
    core.read_8(pc_val, &mut smbf).unwrap();
    println!("bkpt instr: {:?}", &mut smbf);
    println!("pc {:#010x}", pc_val);

    core.write_core_reg(pc.into(), step_pc).unwrap();
    core.step().unwrap();
}

fn run_from_breakpoint(core: &mut Core) {
    step_from_breakpoint(core);
    core.run().unwrap();
}

fn rtt_print(mut rtt: Rtt) {
    let mut channel = rtt.up_channels().take(0);
    loop {
        match &mut channel {
            Some(input) => {
                let mut buf = [0u8; 64];
                let count = input.read(&mut buf[..]).unwrap();
                if count != 0 {
                    println!("Read data: {:?}", str::from_utf8(&buf[..count]).unwrap());
                }
            }
            None => (),
        }
    }
}

fn main() {
    let opts = cli::get_cli_opts();
    probe_wcet(opts);
}
