use crate::{Machine, RAM_BASE, RAM_SIZE};
use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct Symbol {
    pub name: String,
    pub address: u32,
    pub size: u32,
    pub kind: u8,
}

fn u16_at(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let end = offset.checked_add(2).ok_or("ELF offset overflow")?;
    let data = bytes.get(offset..end).ok_or("Truncated ELF header")?;
    Ok(u16::from_le_bytes(data.try_into().unwrap()))
}
fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let end = offset.checked_add(4).ok_or("ELF offset overflow")?;
    let data = bytes.get(offset..end).ok_or("Truncated ELF header")?;
    Ok(u32::from_le_bytes(data.try_into().unwrap()))
}

pub fn load(bytes: &[u8]) -> Result<Machine, String> {
    if bytes.len() < 52 || &bytes[..4] != b"\x7fELF" {
        return Err("Expected an ELF executable".into());
    }
    if bytes[4] != 1 || bytes[5] != 1 || bytes[6] != 1 {
        return Err("Expected little-endian ELF32".into());
    }
    if u16_at(bytes, 18)? != 243 || u16_at(bytes, 16)? != 2 {
        return Err("Expected a statically linked RISC-V executable".into());
    }
    if u32_at(bytes, 36)? & 0x1f != 0 {
        return Err(
            "Compile for RV32IM / ILP32 (no compressed, floating-point ABI, or RVE)".into(),
        );
    }
    let phoff = u32_at(bytes, 28)? as usize;
    let phsize = u16_at(bytes, 42)? as usize;
    let phnum = u16_at(bytes, 44)? as usize;
    if phsize < 32 || phnum == 0 || phnum > 128 {
        return Err("Invalid ELF program headers".into());
    }
    let phend = phnum
        .checked_mul(phsize)
        .and_then(|size| phoff.checked_add(size))
        .ok_or("ELF program header overflow")?;
    if phend > bytes.len() {
        return Err("Truncated ELF program headers".into());
    }
    let mut machine = Machine::empty();
    for i in 0..phnum {
        let at = phoff.checked_add(i * phsize).ok_or("ELF header overflow")?;
        let kind = u32_at(bytes, at)?;
        if kind == 3 || kind == 2 {
            return Err("Dynamically linked executables are not supported".into());
        }
        if kind != 1 {
            continue;
        }
        let offset = u32_at(bytes, at + 4)? as usize;
        let address = u32_at(bytes, at + 8)?;
        let filesz = u32_at(bytes, at + 16)? as usize;
        let memsz = u32_at(bytes, at + 20)? as usize;
        let flags = u32_at(bytes, at + 24)?;
        if filesz > memsz || address < RAM_BASE {
            return Err("Invalid ELF segment".into());
        }
        let start = (address - RAM_BASE) as usize;
        if memsz > RAM_SIZE || start > RAM_SIZE - memsz {
            return Err("ELF segment lies outside simulated RAM".into());
        }
        let end = offset.checked_add(filesz).ok_or("ELF segment overflow")?;
        let data = bytes.get(offset..end).ok_or("Truncated ELF segment")?;
        machine.ram[start..start + memsz].fill(0);
        machine.ram[start..start + filesz].copy_from_slice(data);
        if flags & 1 != 0 {
            machine.executable.push((address, address + memsz as u32));
        }
    }
    machine.pc = u32_at(bytes, 24)?;
    if machine.pc & 3 != 0
        || !machine
            .executable
            .iter()
            .any(|&(a, b)| machine.pc >= a && machine.pc.checked_add(4).is_some_and(|end| end <= b))
    {
        return Err("ELF entry point is outside aligned executable memory".into());
    }
    machine.regs[2] = RAM_BASE + RAM_SIZE as u32 - 16;
    let shoff = u32_at(bytes, 32)? as usize;
    let shsize = u16_at(bytes, 46)? as usize;
    let shnum = u16_at(bytes, 48)? as usize;
    // Symbols are optional metadata. Bad metadata is rejected, never executed.
    if shnum > 0 && shsize < 40 {
        return Err("Invalid ELF section headers".into());
    }
    if shnum > 0 {
        let shend = shnum
            .checked_mul(shsize)
            .and_then(|size| shoff.checked_add(size))
            .ok_or("ELF section header overflow")?;
        if shend > bytes.len() {
            return Err("Truncated ELF section headers".into());
        }
    }
    for i in 0..shnum {
        let at = shoff
            .checked_add(i * shsize)
            .ok_or("ELF section overflow")?;
        if u32_at(bytes, at + 4)? != 2 {
            continue;
        }
        let offset = u32_at(bytes, at + 16)? as usize;
        let size = u32_at(bytes, at + 20)? as usize;
        let link = u32_at(bytes, at + 24)? as usize;
        let entsize = u32_at(bytes, at + 36)? as usize;
        if entsize < 16 || link >= shnum {
            return Err("Invalid ELF symbol table".into());
        }
        let strings = shoff
            .checked_add(link * shsize)
            .ok_or("ELF string overflow")?;
        let stroff = u32_at(bytes, strings + 16)? as usize;
        let strsize = u32_at(bytes, strings + 20)? as usize;
        let strend = stroff.checked_add(strsize).ok_or("ELF string overflow")?;
        let names = bytes.get(stroff..strend).ok_or("Invalid ELF strings")?;
        if offset.checked_add(size).is_none_or(|end| end > bytes.len()) {
            return Err("Truncated ELF symbols".into());
        }
        for n in 0..size / entsize {
            let sym = offset + n * entsize;
            let ni = u32_at(bytes, sym)? as usize;
            let address = u32_at(bytes, sym + 4)?;
            let kind = bytes[sym + 12] & 15;
            if !matches!(kind, 1 | 2) || ni >= names.len() || u16_at(bytes, sym + 14)? == 0 {
                continue;
            }
            let name =
                String::from_utf8_lossy(names[ni..].split(|&b| b == 0).next().unwrap_or_default())
                    .into_owned();
            machine.symbols.push(Symbol {
                name,
                address,
                size: u32_at(bytes, sym + 8)?,
                kind,
            });
        }
    }
    machine.symbols.sort_by_key(|s| s.address);
    Ok(machine)
}
