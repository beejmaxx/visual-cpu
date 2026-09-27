use std::{
    env, fs,
    io::{self, Write},
    process,
};
use visual_cpu::Machine;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: viscpu PROGRAM.elf [MAX_CYCLES]");
        process::exit(2);
    }
    let limit = args
        .get(2)
        .and_then(|x| x.parse::<u64>().ok())
        .unwrap_or(20_000_000);
    let result = (|| -> Result<u32, String> {
        let binary = fs::read(&args[1]).map_err(|e| e.to_string())?;
        let mut cpu = Machine::from_elf(&binary)?;
        let mut printed = 0;
        while !cpu.halted && cpu.cycle < limit {
            cpu.tick();
            if cpu.console.len() > printed {
                io::stdout()
                    .write_all(&cpu.console[printed..])
                    .map_err(|e| e.to_string())?;
                printed = cpu.console.len();
            }
        }
        if let Some(error) = cpu.fault {
            return Err(error);
        }
        if !cpu.halted {
            return Err(format!("Cycle limit {limit} reached"));
        }
        eprintln!(
            "{} instructions · {} modeled cycles",
            cpu.retired, cpu.cycle
        );
        Ok(cpu.exit_code.unwrap_or(0))
    })();
    match result {
        Ok(code) => process::exit((code & 255) as i32),
        Err(e) => {
            eprintln!("viscpu: {e}");
            process::exit(1);
        }
    }
}
