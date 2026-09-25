/* Coroutine contexts on Windows, built on fibers. */
#ifdef _WIN32

#include "context.h"
#include "sched.h"

#include <stdio.h>
#include <stdlib.h>
#include <windows.h>

static LPVOID s_main_fiber = NULL;
static LPVOID s_fibers[SIRIN_MAX_COROUTINES];

static VOID WINAPI fiber_entry(LPVOID unused) {
    (void)unused;
    sirin_sched_enter();
}

void sirin_ctx_init(void) {
    s_main_fiber = ConvertThreadToFiber(NULL);
    if (!s_main_fiber) {
        fprintf(stderr, "sirin runtime: cannot start the async loop (ConvertThreadToFiber failed)\n");
        exit(1);
    }
}

void sirin_ctx_create(int id) {
    s_fibers[id] = CreateFiber(SIRIN_STACK_SIZE, fiber_entry, NULL);
    if (!s_fibers[id]) {
        fprintf(stderr, "sirin runtime: cannot create coroutine (CreateFiber failed)\n");
        exit(1);
    }
}

void sirin_ctx_resume(int id)  { SwitchToFiber(s_fibers[id]); }
void sirin_ctx_suspend(int id) { (void)id; SwitchToFiber(s_main_fiber); }

#endif /* _WIN32 */
