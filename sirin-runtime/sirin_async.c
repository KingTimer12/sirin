/* Must be first — macOS ucontext requires _XOPEN_SOURCE before any system header */
#if !defined(_WIN32) && !defined(_XOPEN_SOURCE)
#define _XOPEN_SOURCE 600
#endif

#include "sirin_async.h"
#include <stdlib.h>
#include <string.h>
#include <stdio.h>

/* ── Channel implementation (shared between all platforms) ─────────────────── */

#define SIRIN_CHAN_CAP 64

struct SirinChannel {
    void* buf[SIRIN_CHAN_CAP];
    int   head;
    int   tail;
    int   size;
};

SirinChannel* sirin_channel_new(void) {
    SirinChannel* ch = (SirinChannel*)malloc(sizeof(SirinChannel));
    ch->head = 0;
    ch->tail = 0;
    ch->size = 0;
    return ch;
}

void sirin_channel_free(SirinChannel* ch) {
    free(ch);
}

/* ── Platform-specific coroutine scheduler ─────────────────────────────────── */

#ifdef _WIN32
/* ============================================================
   Windows — Fibers
   ============================================================ */
#include <windows.h>

static LPVOID           s_main_fiber = NULL;
static LPVOID           s_coro_fiber[SIRIN_MAX_COROUTINES];
static SirinCoroutineFn s_fn[SIRIN_MAX_COROUTINES];
static void*            s_arg[SIRIN_MAX_COROUTINES];
static int              s_done[SIRIN_MAX_COROUTINES];
static int              s_count   = 0;
static int              s_current = -1; /* -1 = main */

static VOID WINAPI coro_entry(LPVOID lpParam) {
    (void)lpParam;
    int id = s_current;
    s_fn[id](s_arg[id]);
    s_done[id] = 1;
    SwitchToFiber(s_main_fiber);
}

void sirin_loop_init(void) {
    setvbuf(stdout, NULL, _IOLBF, 0);  /* real-time output under pipes */
    s_main_fiber = ConvertThreadToFiber(NULL);
    s_count   = 0;
    s_current = -1;
    memset(s_done, 0, sizeof(s_done));
}

void sirin_loop_run(void) {
    int any;
    do {
        any = 0;
        for (int i = 0; i < s_count; i++) {
            if (!s_done[i]) {
                any       = 1;
                s_current = i;
                SwitchToFiber(s_coro_fiber[i]);
                s_current = -1;
            }
        }
    } while (any);
}

void sirin_spawn(SirinCoroutineFn fn, void* arg) {
    int id     = s_count++;
    s_fn[id]   = fn;
    s_arg[id]  = arg;
    s_done[id] = 0;
    s_coro_fiber[id] = CreateFiber(SIRIN_STACK_SIZE, coro_entry, NULL);
}

void sirin_yield(void) {
    if (s_current >= 0) {
        SwitchToFiber(s_main_fiber);
    }
}

int sirin_in_coroutine(void) {
    return s_current >= 0;
}

void sirin_channel_send(SirinChannel* ch, void* value) {
    while (ch->size >= SIRIN_CHAN_CAP) { sirin_yield(); }
    ch->buf[ch->tail] = value;
    ch->tail = (ch->tail + 1) % SIRIN_CHAN_CAP;
    ch->size++;
    sirin_yield();
}

void* sirin_channel_recv(SirinChannel* ch) {
    while (ch->size == 0) { sirin_yield(); }
    void* val = ch->buf[ch->head];
    ch->head  = (ch->head + 1) % SIRIN_CHAN_CAP;
    ch->size--;
    return val;
}

#else
/* ============================================================
   POSIX — ucontext  (Linux / macOS)
   ============================================================ */

#include <ucontext.h>

static ucontext_t       s_main_ctx;
static ucontext_t       s_coro_ctx[SIRIN_MAX_COROUTINES];
static char             s_stack[SIRIN_MAX_COROUTINES][SIRIN_STACK_SIZE];
static SirinCoroutineFn s_fn[SIRIN_MAX_COROUTINES];
static void*            s_arg[SIRIN_MAX_COROUTINES];
static int              s_done[SIRIN_MAX_COROUTINES];
static int              s_count   = 0;
static int              s_current = -1;

static void coro_entry(void) {
    int id = s_current;
    s_fn[id](s_arg[id]);
    s_done[id] = 1;
    swapcontext(&s_coro_ctx[id], &s_main_ctx);
}

void sirin_loop_init(void) {
    setvbuf(stdout, NULL, _IOLBF, 0);  /* real-time output under pipes */
    s_count   = 0;
    s_current = -1;
    memset(s_done, 0, sizeof(s_done));
}

void sirin_loop_run(void) {
    int any;
    do {
        any = 0;
        for (int i = 0; i < s_count; i++) {
            if (!s_done[i]) {
                any       = 1;
                s_current = i;
                swapcontext(&s_main_ctx, &s_coro_ctx[i]);
                s_current = -1;
            }
        }
    } while (any);
}

void sirin_spawn(SirinCoroutineFn fn, void* arg) {
    int id     = s_count++;
    s_fn[id]   = fn;
    s_arg[id]  = arg;
    s_done[id] = 0;

    getcontext(&s_coro_ctx[id]);
    s_coro_ctx[id].uc_stack.ss_sp   = s_stack[id];
    s_coro_ctx[id].uc_stack.ss_size = SIRIN_STACK_SIZE;
    s_coro_ctx[id].uc_link          = NULL;
    makecontext(&s_coro_ctx[id], coro_entry, 0);
}

void sirin_yield(void) {
    if (s_current >= 0) {
        int id    = s_current;
        s_current = -1;
        swapcontext(&s_coro_ctx[id], &s_main_ctx);
        s_current = id;
    }
}

int sirin_in_coroutine(void) {
    return s_current >= 0;
}

void sirin_channel_send(SirinChannel* ch, void* value) {
    while (ch->size >= SIRIN_CHAN_CAP) { sirin_yield(); }
    ch->buf[ch->tail] = value;
    ch->tail = (ch->tail + 1) % SIRIN_CHAN_CAP;
    ch->size++;
    sirin_yield();
}

void* sirin_channel_recv(SirinChannel* ch) {
    while (ch->size == 0) { sirin_yield(); }
    void* val = ch->buf[ch->head];
    ch->head  = (ch->head + 1) % SIRIN_CHAN_CAP;
    ch->size--;
    return val;
}

#endif /* _WIN32 */
