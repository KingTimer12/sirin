#include "channel.h"
#include "sched.h"
#include "../core/mem.h"

#include <stdlib.h>

struct SirinChannel {
    void* buf[SIRIN_CHAN_CAP];
    int   head;
    int   tail;
    int   size;
};

SirinChannel* sirin_channel_new(void) {
    SirinChannel* ch = (SirinChannel*)sirin_alloc(sizeof(SirinChannel));
    ch->head = 0;
    ch->tail = 0;
    ch->size = 0;
    return ch;
}

void sirin_channel_free(SirinChannel* ch) {
    free(ch);
}

void sirin_channel_send(SirinChannel* ch, void* value) {
    while (ch->size >= SIRIN_CHAN_CAP) { sirin_yield(); }
    ch->buf[ch->tail] = value;
    ch->tail = (ch->tail + 1) % SIRIN_CHAN_CAP;
    ch->size++;
    sirin_yield();  /* let a waiting receiver run */
}

void* sirin_channel_recv(SirinChannel* ch) {
    while (ch->size == 0) { sirin_yield(); }
    void* val = ch->buf[ch->head];
    ch->head  = (ch->head + 1) % SIRIN_CHAN_CAP;
    ch->size--;
    return val;
}
