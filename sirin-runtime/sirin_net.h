/* sirin.net: TCP and UDP sockets. Needs sirin.async, since waiting on a
   socket yields to other coroutines. */
#ifndef SIRIN_NET_H
#define SIRIN_NET_H

#include "sirin_runtime.h"
#include "net/tcp.h"
#include "net/udp.h"

/* Called by the generated main before any socket is used. */
void sirin_net_init(void);

#endif
