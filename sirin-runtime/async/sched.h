/* Cooperative coroutine scheduler behind `spawn`, `async fn` and `.await`.
   Coroutines run on one thread and switch only when they call sirin_yield. */
#ifndef SIRIN_ASYNC_SCHED_H
#define SIRIN_ASYNC_SCHED_H

#define SIRIN_STACK_SIZE     65536
#define SIRIN_MAX_COROUTINES 1024

typedef void (*SirinCoroutineFn)(void*);

void sirin_loop_init(void);   /* call once, before the first spawn     */
void sirin_loop_run(void);    /* run until every coroutine has finished */
void sirin_spawn(SirinCoroutineFn fn, void* arg);
void sirin_yield(void);       /* give the other coroutines a turn       */
int  sirin_in_coroutine(void);

#endif
