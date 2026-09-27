#include "runtime.h"

int values[] = {42, 7, 91, 3, 64, 18, 55, 2, 76, 11, 29, 5};

void sort(int *a, int lo, int hi) {
    if (lo >= hi) return;
    int pivot = a[hi], p = lo;
    for (int j = lo; j < hi; ++j) {
        if (a[j] < pivot) {
            int t = a[p]; a[p] = a[j]; a[j] = t; ++p;
        }
    }
    int t = a[p]; a[p] = a[hi]; a[hi] = t;
    sort(a, lo, p - 1); sort(a, p + 1, hi);
}

int main(void) {
    sort(values, 0, 11);
    print("Quicksort\n");
    for (int i = 0; i < 12; ++i) {print_u32((unsigned)values[i]); put_char(i == 11 ? '\n' : ' ');}
    return 0;
}
