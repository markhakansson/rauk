use probe_rs::{Core, MemoryInterface};

pub fn step_from_breakpoint(core: &mut Core) {
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

pub fn run_from_breakpoint(core: &mut Core) {
    step_from_breakpoint(core);
    core.run().unwrap();
}
