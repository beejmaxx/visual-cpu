#!/usr/bin/env python3
"""Build actual RV32IM guest executables and the Rust/WASM browser engine."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]

def run(args):
    print("+", " ".join(map(str, args)), flush=True)
    subprocess.run(list(map(str, args)), cwd=ROOT, check=True)

def guests():
    candidates = [os.environ.get("RISCV_CLANG"), "/opt/homebrew/opt/llvm/bin/clang", shutil.which("clang")]
    compiler = None
    for candidate in candidates:
        if candidate and Path(candidate).exists():
            targets = subprocess.run([candidate, "--print-targets"], capture_output=True, text=True, check=True).stdout
            if "riscv32" in targets:
                compiler = candidate
                break
    if not compiler:
        raise SystemExit("Install LLVM with the RISC-V backend, or set RISCV_CLANG to its clang binary.")
    linker = os.environ.get("RISCV_LLD") or shutil.which("ld.lld")
    if not linker:
        raise SystemExit("Install LLD, or set RISCV_LLD to ld.lld.")
    out = ROOT / "web/programs"
    out.mkdir(parents=True, exist_ok=True)
    for name in ("flow", "alu", "array_sum", "fibonacci", "sort", "echo"):
        source = f"{name}.S" if name == "alu" else f"{name}.c"
        run([compiler, "--target=riscv32-none-elf", "-march=rv32im", "-mabi=ilp32",
             "-msmall-data-limit=0", "-mno-relax", "-ffreestanding", "-fno-builtin", "-nostdlib",
             "-O1", "-g", "-fno-omit-frame-pointer", f"--ld-path={linker}",
             "-Wl,-T,guest/link.ld", "-Wl,--no-relax", "-Wl,--build-id=none",
             "guest/start.S", "guest/runtime.c", f"guest/{source}", "-o", out / f"{name}.elf"])
        shutil.copy(ROOT / f"guest/{source}", out / source)

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--guest-only", action="store_true")
    args = parser.parse_args()
    guests()
    if not args.guest_only:
        run(["cargo", "build", "--release", "--target", "wasm32-unknown-unknown", "--lib"])
        run(["wasm-bindgen", "--target", "web", "--out-dir", "web/pkg",
             "target/wasm32-unknown-unknown/release/visual_cpu.wasm"])
