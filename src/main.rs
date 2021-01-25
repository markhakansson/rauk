use probe_rs::{
    Probe,
    Session,
    flashing::{download_file, Format},
    MemoryInterface
};
use std::path::PathBuf;
use structopt::StructOpt;
use std::sync::{Arc, Mutex};
use probe_rs_rtt::Rtt;
use std::str;

#[derive(Debug, StructOpt)]
struct Cli {
    #[structopt(parse(from_os_str))]
    path: PathBuf,
    #[structopt(short, long)]
    rtt: bool,
    #[structopt(short, long)]
    wcet: bool,
    #[structopt(short, long)]
    gdb: bool
}

fn probe_wcet(path: PathBuf, rtt_enable: bool, wcet: bool, gdb: bool) {
    // Get a list of all available debug probes.
    let probes = Probe::list_all();
    println!("{:?}", probes);

    // Use the first probe found.
    let probe = probes[0].open().unwrap();

    // Attach to a chip.
    let mut session = probe.attach("STM32F401RETx").unwrap();

    // Flash the card with binary
    download_file(
        &mut session,
        path.as_path(),
        Format::Elf
    ).expect("Could not flash the card");
    println!("Card flashed");

    // Reset the core and halt
    {
        let mut core = session.core(0).unwrap();
        core.reset_and_halt(std::time::Duration::from_secs(1)).unwrap();
        core.wait_for_core_halted(std::time::Duration::from_secs(5)).unwrap();
    }

    println!("Create mutex");
    let session = Arc::new(Mutex::new(session));

    let mut gdb_thread_handle = None;
    if gdb {
        println!("Starting gdb server");
        let session  = session.clone();
        gdb_thread_handle = Some(std::thread::spawn(move || {
            let gdb_connection_string = "127.0.0.1:1337";
            if let Err(e) = probe_rs_gdb_server::run(Some(gdb_connection_string), &session) {
                println!("{:?}", e);
            };
        }));
    }

    {
        println!("Starting program");
        let mut session = session.lock().unwrap();
        let mut core = session.core(0).unwrap();
        let pc = core.registers().program_counter();

        if wcet {
            core.run().unwrap();
            core.wait_for_core_halted(std::time::Duration::from_secs(5)).unwrap();

            let mut buff = [0u32; 1];
            let mut smbf = [0u8; 2];
            let mut pc_val: u32;
            let mut step_pc: u32;

            // First breakpoint
            pc_val = core.read_core_reg(pc).unwrap();
            core.read_32(0xe000_1004, &mut buff).unwrap();
            core.read_8(pc_val, &mut smbf).unwrap();
            println!("bkpt instr: {:?}", &mut smbf);
            println!("cyccnt {:?}", buff);
            println!("pc {:#010x}", pc_val);
            // Continue from this breakpoint to next breakpoint
            step_pc = pc_val + 0x2;
            core.write_core_reg(pc.into(), step_pc).unwrap();
            core.run().unwrap();
            core.wait_for_core_halted(std::time::Duration::from_secs(5)).unwrap();


            // Second breakpoint
            pc_val = core.read_core_reg(pc).unwrap();
            core.read_32(0xe000_1004, &mut buff).unwrap();
            core.read_8(pc_val, &mut smbf).unwrap();
            println!("bkpt instr: {:?}", &mut smbf);
            println!("cyccnt {:?}", buff);
            println!("pc {:#010x}", pc_val);
            // Go to next breakpoint
            step_pc = pc_val + 0x2;
            core.write_core_reg(pc.into(), step_pc).unwrap();
            core.run().unwrap();
            core.wait_for_core_halted(std::time::Duration::from_secs(5)).unwrap();

            // Third breakpoint
            pc_val = core.read_core_reg(pc).unwrap();
            core.read_32(0xe000_1004, &mut buff).unwrap();
            core.read_8(pc_val, &mut smbf).unwrap();
            println!("bkpt instr: {:?}", &mut smbf);
            println!("cyccnt {:?}", buff);
            println!("pc {:#010x}", pc_val);
        }
        if !gdb {
            core.run().unwrap();
        }
    }

    if let Some(gdb_thread_handle) = gdb_thread_handle {
        let _ = gdb_thread_handle.join();
    }

    if rtt_enable {
        println!("Attaching to rtt");
        let rtt = Rtt::attach(session.clone()).unwrap();
        rtt_print(rtt);
    }
}

fn run_from_breakpoint(session: &Mutex<Session>) {
    let mut sess = session.lock().unwrap();
    let mut core = sess.core(0).unwrap();
    let pc = core.registers().program_counter();
    let step_pc = core.read_core_reg(pc).unwrap() + 0x2;
    core.write_core_reg(pc.into(), step_pc).unwrap();
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
            },
            None => (),
        }
    }
}

fn main() {
    let opt = Cli::from_args();
    probe_wcet(opt.path, opt.rtt, opt.wcet, opt.gdb);
}
