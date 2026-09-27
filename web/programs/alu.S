.section .text
.globl main
.type main, @function
main:
    li a1, 5
    li a2, 3
.globl add_example
add_example:
    add a0, a1, a2       # 5 + 3 = 8; carries through bits 0, 1, 2
    sub t0, a0, a2       # 8 - 3 = 5
    xor t1, a0, t0       # 8 XOR 5 = 13
    slli t2, t1, 3       # shift left by 1, then 2 in the barrel shifter
    slt t3, a2, a1       # signed comparison: 3 < 5
    mul t4, a1, a2       # 5 * 3 = 15
    li t3, -17
    div t0, t3, a1      # -17 / 5 = -3; restoring division then sign correction
    rem t1, t3, a1      # -17 % 5 = -2; the remainder keeps the dividend sign
    la t5, values
    lw a3, 0(t5)         # cold data read
    lw a4, 4(t5)         # same 64-byte line: L1D hit
    sw a0, 8(t5)         # write-through store of 8
    lw a5, 8(t5)         # read the stored value
    li a0, 0
    ret
.size main, .-main

.section .data
.balign 64
.globl values
.type values, @object
values:
    .word 42, 99, 0
.size values, .-values
