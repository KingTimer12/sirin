#include "io.h"
#include "mem.h"
#include "../sirin_config.h"

#include <stdio.h>
#include <string.h>

#define LINE_MAX_LEN 1024

/* Blocking read of one line into buf, newline stripped. Returns 0 at EOF. */
static int read_line_blocking(char* buf, size_t cap) {
    if (!fgets(buf, (int)cap, stdin)) { buf[0] = '\0'; return 0; }
    size_t len = strlen(buf);
    if (len > 0 && buf[len - 1] == '\n') buf[--len] = '\0';
    if (len > 0 && buf[len - 1] == '\r') buf[--len] = '\0';
    return 1;
}

/* The line is returned as an owned heap string, like every other `str` the
   runtime hands out: the generated code frees it when it goes out of scope. */
static const char* owned(const char* line) {
    return sirin_dup(line, strlen(line));
}

#ifndef SIRIN_USE_ASYNC

const char* sirin_readln(void) {
    char buf[LINE_MAX_LEN];
    read_line_blocking(buf, sizeof(buf));
    return owned(buf);
}

#else /* SIRIN_USE_ASYNC: wait for input without blocking other coroutines */

#include "../async/sched.h"

/* Fills buf with the next line, yielding while none is available. */
static void read_line_cooperative(char* buf, size_t cap);

const char* sirin_readln(void) {
    char buf[LINE_MAX_LEN];
    if (sirin_in_coroutine()) {
        read_line_cooperative(buf, sizeof(buf));
    } else {
        read_line_blocking(buf, sizeof(buf));
    }
    return owned(buf);
}

#ifdef _WIN32
/* Console and pipe handles can't be polled the same way on Windows, so a
   helper thread does the blocking read and the coroutine polls its result. */
#include <windows.h>

static CRITICAL_SECTION s_lock;
static HANDLE s_wanted;              /* signalled when a coroutine wants a line */
static char   s_line[LINE_MAX_LEN];
static int    s_ready   = 0;         /* s_line holds an unread line            */
static int    s_eof     = 0;
static int    s_pending = 0;         /* a read was requested, not yet consumed */

static DWORD WINAPI reader_thread(LPVOID unused) {
    (void)unused;
    char tmp[LINE_MAX_LEN];
    for (;;) {
        WaitForSingleObject(s_wanted, INFINITE);
        int ok = read_line_blocking(tmp, sizeof(tmp));
        EnterCriticalSection(&s_lock);
        memcpy(s_line, tmp, sizeof(tmp));
        s_eof   = !ok;
        s_ready = 1;
        LeaveCriticalSection(&s_lock);
        if (!ok) return 0;
    }
}

static void read_line_cooperative(char* buf, size_t cap) {
    static int started = 0;
    if (!started) {
        InitializeCriticalSection(&s_lock);
        s_wanted = CreateEventA(NULL, FALSE, FALSE, NULL);
        CreateThread(NULL, 0, reader_thread, NULL, 0, NULL);
        started = 1;
    }
    if (s_eof && !s_ready) { buf[0] = '\0'; return; }
    if (!s_pending) { s_pending = 1; SetEvent(s_wanted); }
    for (;;) {
        EnterCriticalSection(&s_lock);
        int ready = s_ready;
        if (ready) {
            memcpy(buf, s_line, cap < sizeof(s_line) ? cap : sizeof(s_line));
            buf[cap - 1] = '\0';
            s_ready   = 0;
            s_pending = 0;
        }
        LeaveCriticalSection(&s_lock);
        if (ready) return;
        sirin_yield();
    }
}

#else /* POSIX: make stdin non-blocking and yield while no byte is available */
#include <errno.h>
#include <fcntl.h>
#include <unistd.h>

static void read_line_cooperative(char* buf, size_t cap) {
    static int nonblocking = 0;
    if (!nonblocking) {
        int fl = fcntl(0, F_GETFL, 0);
        fcntl(0, F_SETFL, fl | O_NONBLOCK);
        nonblocking = 1;
    }
    size_t len = 0;
    for (;;) {
        char c;
        ssize_t r = read(0, &c, 1);
        if (r > 0) {
            if (c == '\n') break;
            if (len < cap - 1) buf[len++] = c;
        } else if (r == 0) {
            break;  /* EOF */
        } else if (errno == EAGAIN || errno == EWOULDBLOCK) {
            sirin_yield();
        } else {
            break;
        }
    }
    if (len > 0 && buf[len - 1] == '\r') len--;
    buf[len] = '\0';
}
#endif /* _WIN32 */

#endif /* SIRIN_USE_ASYNC */
