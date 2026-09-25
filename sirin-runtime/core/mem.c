#include "mem.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

void* sirin_alloc(size_t n) {
    void* p = malloc(n);
    if (!p) {
        fprintf(stderr, "sirin runtime: out of memory (malloc %lu bytes)\n", (unsigned long)n);
        exit(1);
    }
    return p;
}

void* sirin_realloc(void* p, size_t n) {
    void* q = realloc(p, n);
    if (!q) {
        fprintf(stderr, "sirin runtime: out of memory (realloc %lu bytes)\n", (unsigned long)n);
        exit(1);
    }
    return q;
}

char* sirin_dup(const char* s, size_t n) {
    char* buf = (char*)sirin_alloc(n + 1);
    memcpy(buf, s, n);
    buf[n] = '\0';
    return buf;
}
