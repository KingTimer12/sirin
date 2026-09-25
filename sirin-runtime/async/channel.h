/* Channel[T]: bounded FIFO between coroutines. send waits while the channel
   is full and recv waits while it is empty, yielding to other coroutines. */
#ifndef SIRIN_ASYNC_CHANNEL_H
#define SIRIN_ASYNC_CHANNEL_H

#define SIRIN_CHAN_CAP 64

typedef struct SirinChannel SirinChannel;

SirinChannel* sirin_channel_new(void);
void          sirin_channel_send(SirinChannel* ch, void* value);
void*         sirin_channel_recv(SirinChannel* ch);
void          sirin_channel_free(SirinChannel* ch);

#endif
