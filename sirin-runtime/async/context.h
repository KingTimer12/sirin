/* Internal: the platform part of the scheduler. context_win32.c (fibers) and
   context_posix.c (ucontext) each implement these for their platform; the
   scheduling policy itself lives in sched.c. */
#ifndef SIRIN_ASYNC_CONTEXT_H
#define SIRIN_ASYNC_CONTEXT_H

void sirin_ctx_init(void);        /* turn the calling thread into the main context */
void sirin_ctx_create(int id);    /* new coroutine context that starts in sirin_sched_enter */
void sirin_ctx_resume(int id);    /* main -> coroutine `id`                    */
void sirin_ctx_suspend(int id);   /* coroutine `id` -> main                    */

/* Provided by sched.c: runs the current coroutine's body, then suspends it
   for good. Every coroutine context starts here. */
void sirin_sched_enter(void);

#endif
