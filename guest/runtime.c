#include "runtime.h"

void put_char(char ch) { *(volatile unsigned char *)0x10000000 = (unsigned char)ch; }
void print(const char *text) { while (*text) put_char(*text++); }
void print_u32(unsigned value) {
    char digits[10];
    unsigned length = 0;
    do { digits[length++] = (char)('0' + value % 10); value /= 10; } while (value);
    while (length) put_char(digits[--length]);
}
char get_char(void) {
    while (!*(volatile unsigned *)0x10000008) {}
    return *(volatile unsigned char *)0x10000004;
}
