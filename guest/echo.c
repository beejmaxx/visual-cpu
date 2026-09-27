#include "runtime.h"

int main(void) {
    print("> ");
    for (;;) {
        char ch = get_char();
        put_char(ch);
        if (ch == '\n') break;
    }
    return 0;
}
