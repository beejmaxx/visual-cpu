#include "runtime.h"

// Volatile preserves the loads, multiplication, and stores in the executable.
volatile unsigned samples[4] = {7, 11, 42, 9};
volatile unsigned factor = 3;
volatile unsigned products[4];

int main(void) {
    // Visit index 2 first, while its cache line is cold.
    for (unsigned step = 0; step < 4; ++step) {
        unsigned i = (step + 2) & 3;
        products[i] = samples[i] * factor;
    }

    print_u32(products[2]);
    put_char('\n');
    return 0;
}
