#ifndef SIRIN_ASYNC_H
#define SIRIN_ASYNC_H

#include <stddef.h>

#define SIRIN_STACK_SIZE    65536
#define SIRIN_MAX_COROUTINES 1024

typedef struct SirinCoroutine SirinCoroutine;
typedef void (*SirinCoroutineFn)(void*);

void sirin_loop_init(void);
void sirin_loop_run(void);
void sirin_spawn(SirinCoroutineFn fn, void* arg);
void sirin_yield(void);
int  sirin_in_coroutine(void);

typedef struct SirinChannel SirinChannel;
SirinChannel* sirin_channel_new(void);
void          sirin_channel_send(SirinChannel* ch, void* value);
void*         sirin_channel_recv(SirinChannel* ch);
void          sirin_channel_free(SirinChannel* ch);

#endif /* SIRIN_ASYNC_H */
