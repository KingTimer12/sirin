#include "tcp.h"
#include "sys.h"
#include "../async/sched.h"
#include "../core/mem.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* ── TcpListener ──────────────────────────────────────────────────────────── */

SirinTcpListener sirin_tcp_listener_bind(const char* addr, int port) {
    int fd = (int)socket(AF_INET, SOCK_STREAM, 0);
    if (fd < 0) sirin_net_fail("socket() failed");

    int yes = 1;
    setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, (const char*)&yes, sizeof(yes));

    struct sockaddr_in sa = sirin_net_addr(addr, port);
    if (bind(fd, (struct sockaddr*)&sa, sizeof(sa)) < 0) {
        fprintf(stderr, "sirin_net: bind() failed on %s:%d (is the port already in use?)\n", addr, port);
        exit(1);
    }
    if (listen(fd, 128) < 0) sirin_net_fail("listen() failed");
    sirin_net_set_nonblocking(fd);
    return (SirinTcpListener){ .fd = fd };
}

SirinTcpStream sirin_tcp_listener_accept(SirinTcpListener* l) {
    for (;;) {
        struct sockaddr_in client;
        socklen_t len = sizeof(client);
        int cfd = (int)accept(l->fd, (struct sockaddr*)&client, &len);
        if (cfd >= 0) {
            sirin_net_set_nonblocking(cfd);
            return (SirinTcpStream){ .fd = cfd };
        }
        if (!SIRIN_WOULD_BLOCK()) sirin_net_fail("accept() failed");
        sirin_yield();
    }
}

void sirin_tcp_listener_close(SirinTcpListener* l) {
    SIRIN_SOCK_CLOSE(l->fd);
    l->fd = -1;
}

SirinTcpListener sirin_tcp_listener_clone(SirinTcpListener* l) {
    return (SirinTcpListener){ .fd = sirin_net_dup(l->fd) };
}

/* ── TcpStream ────────────────────────────────────────────────────────────── */

SirinTcpStream sirin_tcp_stream_connect(const char* addr, int port) {
    int fd = (int)socket(AF_INET, SOCK_STREAM, 0);
    if (fd < 0) sirin_net_fail("socket() failed");
    struct sockaddr_in sa = sirin_net_addr(addr, port);
    if (connect(fd, (struct sockaddr*)&sa, sizeof(sa)) < 0) {
        fprintf(stderr, "sirin_net: connect() failed to %s:%d (is the server running?)\n", addr, port);
        exit(1);
    }
    sirin_net_set_nonblocking(fd);
    return (SirinTcpStream){ .fd = fd };
}

const char* sirin_tcp_stream_read(SirinTcpStream* s) {
    char* buf = (char*)sirin_alloc(SIRIN_TCP_READ_BUF);
    for (;;) {
        int n = (int)recv(s->fd, buf, SIRIN_TCP_READ_BUF - 1, 0);
        if (n >= 0) {
            buf[n] = '\0';  /* n == 0: peer closed, read as "" */
            return buf;
        }
        if (!SIRIN_WOULD_BLOCK()) {
            /* Reset or aborted connection: the peer is gone, which a server
               must survive, so it reads as closed too. */
            buf[0] = '\0';
            return buf;
        }
        sirin_yield();
    }
}

void sirin_tcp_stream_write(SirinTcpStream* s, const char* data) {
    size_t total = strlen(data);
    size_t sent  = 0;
    while (sent < total) {
        int n = (int)send(s->fd, data + sent, (int)(total - sent), 0);
        if (n < 0) {
            if (SIRIN_WOULD_BLOCK()) { sirin_yield(); continue; }
            return;  /* broken pipe / closed peer: drop the data, not fatal */
        }
        if (n == 0) return;
        sent += (size_t)n;
    }
}

void sirin_tcp_stream_close(SirinTcpStream* s) {
    SIRIN_SOCK_CLOSE(s->fd);
    s->fd = -1;
}

SirinTcpStream sirin_tcp_stream_clone(SirinTcpStream* s) {
    return (SirinTcpStream){ .fd = sirin_net_dup(s->fd) };
}
