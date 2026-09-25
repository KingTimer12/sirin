/* Coroutine contexts on Linux and macOS, built on ucontext. */
#ifndef _WIN32

/* Must come before any system header: macOS only declares ucontext with it. */
#ifndef _XOPEN_SOURCE
#define _XOPEN_SOURCE 600
#endif

#include "context.h"
#include "sched.h"

#include <ucontext.h>

static ucontext_t s_main_ctx;
static ucontext_t s_ctx[SIRIN_MAX_COROUTINES];
static char       s_stack[SIRIN_MAX_COROUTINES][SIRIN_STACK_SIZE];

void sirin_ctx_init(void) {}

void sirin_ctx_create(int id) {
    getcontext(&s_ctx[id]);
    s_ctx[id].uc_stack.ss_sp   = s_stack[id];
    s_ctx[id].uc_stack.ss_size = SIRIN_STACK_SIZE;
    s_ctx[id].uc_link          = NULL;
    makecontext(&s_ctx[id], sirin_sched_enter, 0);
}

void sirin_ctx_resume(int id)  { swapcontext(&s_main_ctx, &s_ctx[id]); }
void sirin_ctx_suspend(int id) { swapcontext(&s_ctx[id], &s_main_ctx); }

#endif /* !_WIN32 */
