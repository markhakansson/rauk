use anyhow::{Context, Result};
use std::{
    collections::HashMap,
    path::PathBuf,
    process::{Command, ExitStatus},
};

/// The results/output of llvm-objdump on the rtic binary
#[derive(Debug, Clone)]
pub struct Objdump {
    instructions: HashMap<u64, String>,
}

impl Objdump {
    /// Returns the instruction at the given address if it exists
    pub fn get_instruction(&self, address: &u64) -> Option<String> {
        if let Some(instruction) = self.instructions.get(address) {
            Some(instruction.clone())
        } else {
            None
        }
    }
}

/// Disassembles a binary at `binary_path` using `llvm-bojdump`
pub fn disassemble(binary_path: &PathBuf) -> Result<Objdump> {
    let mut objdump = Command::new("llvm-objdump");

    objdump
        .arg("--disassemble")
        .arg("--print-imm-hex")
        .arg("--no-show-raw-insn")
        .arg(binary_path.to_str().unwrap());

    let output = objdump.output()?;

    let result = String::from_utf8(output.stdout)?;
    let iter = result
        .split("\n")
        .filter(|x| !x.is_empty())
        .map(|x| x.replace("\t", " "));

    let mut map: HashMap<u64, String> = HashMap::new();

    // find and add only addresses/instructions to the map
    for i in iter {
        let line = i.trim();
        if line.starts_with("8") {
            if let Some(index) = line.find(":") {
                let (address, instruction) = line.split_at(index);
                let instruction = instruction.strip_prefix(":").unwrap();
                let instruction = instruction.trim();
                let address = u64::from_str_radix(address, 16)?;
                map.insert(address, instruction.to_string());
            }
        }
    }

    let result = Objdump { instructions: map };

    Ok(result)
}
