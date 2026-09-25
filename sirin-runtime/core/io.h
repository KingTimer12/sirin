/* sirin.io — console input. Output goes straight through printf. */
#ifndef SIRIN_CORE_IO_H
#define SIRIN_CORE_IO_H

#include <stdio.h>
#include <string.h>

/* One line from stdin without the newline, or "" at end of input. Inside a
   coroutine it waits cooperatively, so other coroutines keep running. */
const char* sirin_readln(void);

#endif
