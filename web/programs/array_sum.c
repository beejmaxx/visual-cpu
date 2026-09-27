#include "runtime.h"

// Volatile keeps both passes visible in the compiled program.
const volatile unsigned numbers[16] = {
    1, 2, 3, 4, 5, 6, 7, 8,
    9, 10, 11, 12, 13, 14, 15, 16
};
volatile unsigned results[2];

int main(void) {
    for (unsigned pass = 0; pass < 2; ++pass) {
        unsigned sum = 0;
        for (unsigned i = 0; i < 16; ++i) sum += numbers[i];
        results[pass] = sum;
    }
    print("First pass:  "); print_u32(results[0]); put_char('\n');
    print("Second pass: "); print_u32(results[1]); put_char('\n');
    return 0;
}
