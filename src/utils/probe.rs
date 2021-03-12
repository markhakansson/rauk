use anyhow::Result;
use probe_rs::{Core, MemoryInterface};

const CYCCNT: u32 = 0xe000_1004;

pub fn step_from_breakpoint(core: &mut Core) -> Result<()> {
    let mut smbf = [0u8; 2];
    let pc = core.registers().program_counter();
    let pc_val = core.read_core_reg(pc)?;
    let step_pc = pc_val + 0x2;

    core.read_8(pc_val, &mut smbf)?;

    core.write_core_reg(pc.into(), step_pc)?;
    core.step()?;
    Ok(())
}

pub fn run(core: &mut Core) -> Result<()> {
    if core.core_halted()? {
        if breakpoint_at_pc(core)? {
            step_from_breakpoint(core)?;
        }
    }
    core.run()?;
    Ok(())
}

pub fn breakpoint_at_pc(core: &mut Core) -> Result<bool> {
    let mut instr16 = [0u8; 2];
    let pc = core.registers().program_counter();
    let pc_val = core.read_core_reg(pc)?;
    core.read_8(pc_val, &mut instr16)?;

    let check = match instr16[1] {
        0b10111110 => true,
        _ => false,
    };
    Ok(check)
}

pub fn read_breakpoint_value(core: &mut Core) -> Result<Option<u8>> {
    let mut instr16 = [0u8; 2];
    let pc = core.registers().program_counter();
    let pc_val = core.read_core_reg(pc)?;
    core.read_8(pc_val, &mut instr16)?;

    let value = match instr16[1] {
        0b10111110 => Some(instr16[0]),
        _ => None,
    };
    Ok(value)
}

pub fn read_cycle_counter(core: &mut Core) -> Result<u32> {
    let mut buf = [0u32, 1];
    core.read_32(CYCCNT, &mut buf)?;
    Ok(buf[0])
}
