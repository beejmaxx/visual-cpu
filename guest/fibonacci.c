#include "runtime.h"

unsigned fib(unsigned n) {
    if (n < 2) return n;
    return fib(n - 1) + fib(n - 2);
}

int main(void) {
    print("Recursive Fibonacci\n");
    for (unsigned i = 0; i < 10; ++i) {
        print_u32(fib(i)); put_char(i == 9 ? '\n' : ' ');
    }
    return 0;
}
