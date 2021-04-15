use anyhow::{anyhow, Result};
use probe_rs::{Core, MemoryInterface, Probe, Session};

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

/// Wrapper around probe::core.run(). But also continues
/// if there is a breakpoint at the current program counter.
pub fn run(core: &mut Core) -> Result<()> {
    if core.core_halted()? {
        if breakpoint_at_pc(core)? {
            step_from_breakpoint(core)?;
        }
    }
    core.run()?;
    Ok(())
}

/// Checks if there is a breakpoint at the current program counter.
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

pub fn read_breakpoint_value(core: &mut Core) -> Result<u8> {
    let mut instr16 = [0u8; 2];
    let pc = core.registers().program_counter();
    let pc_val = core.read_core_reg(pc)?;
    core.read_8(pc_val, &mut instr16)?;

    match instr16[1] {
        0b10111110 => Ok(instr16[0]),
        _ => Err(anyhow!(
            "Not a breakpoint instruction at current PC: {:x?}",
            pc_val
        )),
    }
}

pub fn read_cycle_counter(core: &mut Core) -> Result<u32> {
    let mut buf = [0u32, 1];
    core.read_32(CYCCNT, &mut buf)?;
    Ok(buf[0])
}

/// Opens the first probe it can find and return its session
pub fn open_and_attach_probe(chip_name: &String) -> Result<Session> {
    let probes = Probe::list_all();

    if probes.is_empty() {
        return Err(anyhow!("There are no debug probes connected"));
    } else {
        let probe = probes[0].open()?;
        Ok(probe.attach(chip_name)?)
    }
}
