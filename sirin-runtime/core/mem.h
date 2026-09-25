/* Allocation helpers shared by the whole runtime. They never return NULL:
   running out of memory aborts the program with a message. */
#ifndef SIRIN_CORE_MEM_H
#define SIRIN_CORE_MEM_H

#include <stddef.h>

void* sirin_alloc(size_t n);
void* sirin_realloc(void* p, size_t n);

/* Heap copy of the first n bytes of s, NUL-terminated. The caller owns it. */
char* sirin_dup(const char* s, size_t n);

#endif
