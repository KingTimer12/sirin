#include "sched.h"
#include "context.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static SirinCoroutineFn s_fn[SIRIN_MAX_COROUTINES];
static void*            s_arg[SIRIN_MAX_COROUTINES];
static int              s_done[SIRIN_MAX_COROUTINES];
static int              s_count   = 0;
static int              s_current = -1; /* -1 = main */

void sirin_loop_init(void) {
    /* Real-time output under pipes: a long-running loop must not sit on a
       full buffer. The Windows CRT treats _IOLBF as full buffering, so it
       gets no buffering at all instead. */
#ifdef _WIN32
    setvbuf(stdout, NULL, _IONBF, 0);
#else
    setvbuf(stdout, NULL, _IOLBF, 0);
#endif
    sirin_ctx_init();
    s_count   = 0;
    s_current = -1;
    memset(s_done, 0, sizeof(s_done));
}

/* Round-robin: resume every unfinished coroutine until none is left. */
void sirin_loop_run(void) {
    int any;
    do {
        any = 0;
        for (int i = 0; i < s_count; i++) {
            if (!s_done[i]) {
                any       = 1;
                s_current = i;
                sirin_ctx_resume(i);
                s_current = -1;
            }
        }
    } while (any);
}

void sirin_spawn(SirinCoroutineFn fn, void* arg) {
    if (s_count >= SIRIN_MAX_COROUTINES) {
        fprintf(stderr, "sirin runtime: too many coroutines (limit %d)\n", SIRIN_MAX_COROUTINES);
        exit(1);
    }
    int id     = s_count++;
    s_fn[id]   = fn;
    s_arg[id]  = arg;
    s_done[id] = 0;
    sirin_ctx_create(id);
}

void sirin_yield(void) {
    if (s_current < 0) return;
    int id    = s_current;
    s_current = -1;
    sirin_ctx_suspend(id);
    s_current = id;
}

int sirin_in_coroutine(void) {
    return s_current >= 0;
}

void sirin_sched_enter(void) {
    int id = s_current;
    s_fn[id](s_arg[id]);
    s_done[id] = 1;
    sirin_ctx_suspend(id);  /* never resumed again: the loop skips done ones */
}
