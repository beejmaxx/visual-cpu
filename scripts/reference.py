#!/usr/bin/env python3
"""Compare complete program output with independently executed QEMU RV32 code."""
from pathlib import Path
import os
import selectors
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
qemu = shutil.which("qemu-system-riscv32")
if not qemu:
    raise SystemExit("qemu-system-riscv32 is required for this optional reference check")
subprocess.run(["cargo", "build", "--release", "--bin", "viscpu"], cwd=ROOT, check=True)
for name in ("flow", "array_sum", "fibonacci", "sort"):
    elf = ROOT / f"web/programs/{name}.elf"
    expected = subprocess.run([ROOT / "target/release/viscpu", elf], capture_output=True, check=True).stdout
    process = subprocess.Popen([qemu, "-M", "virt", "-bios", "none", "-display", "none",
                                "-serial", "stdio", "-monitor", "none", "-kernel", elf],
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    selector = selectors.DefaultSelector()
    selector.register(process.stdout, selectors.EVENT_READ)
    actual = b""
    deadline = time.monotonic() + 10
    try:
        while len(actual) < len(expected) and time.monotonic() < deadline:
            if selector.select(0.2):
                data = os.read(process.stdout.fileno(), 65536)
                if not data:
                    break
                actual += data
        assert actual == expected, (name, expected, actual)
        print(f"PASS: {name} output matches QEMU exactly")
    finally:
        process.terminate()
        process.wait(timeout=5)
        selector.close()
