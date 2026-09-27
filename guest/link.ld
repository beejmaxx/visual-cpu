OUTPUT_ARCH(riscv)
ENTRY(_start)
MEMORY { RAM (rwx) : ORIGIN = 0x80000000, LENGTH = 16M }
SECTIONS {
    .text : { *(.text.start) *(.text .text.*) } > RAM
    .rodata ALIGN(64) : { *(.rodata .rodata.*) } > RAM
    .data ALIGN(64) : { *(.data .data.* .sdata .sdata.*) } > RAM
    .bss ALIGN(64) : { *(.bss .bss.* .sbss .sbss.*) *(COMMON) } > RAM
    __stack_top = ORIGIN(RAM) + LENGTH(RAM) - 16;
}
