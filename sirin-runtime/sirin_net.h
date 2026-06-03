#ifndef SIRIN_NET_H
#define SIRIN_NET_H

#include <stddef.h>
#include "sirin_runtime.h"

#define SIRIN_TCP_READ_BUF 4096

/* ── TCP ── */
typedef struct { int fd; } SirinTcpListener;
typedef struct { int fd; } SirinTcpStream;

/* listener */
SirinTcpListener sirin_tcp_listener_bind(const char* addr, int port);
SirinTcpStream   sirin_tcp_listener_accept(SirinTcpListener* l);
void             sirin_tcp_listener_close(SirinTcpListener* l);
SirinTcpListener sirin_tcp_listener_clone(SirinTcpListener* l);

/* stream */
SirinTcpStream   sirin_tcp_stream_connect(const char* addr, int port);
const char*      sirin_tcp_stream_read(SirinTcpStream* s);
void             sirin_tcp_stream_write(SirinTcpStream* s, const char* data);
void             sirin_tcp_stream_close(SirinTcpStream* s);
SirinTcpStream   sirin_tcp_stream_clone(SirinTcpStream* s);

/* ── UDP ── */
typedef struct { int fd; } SirinUdpSocket;
typedef struct {
    const char* data;
    char        addr[64];
    int         port;
} SirinUdpPacket;

SirinUdpSocket  sirin_udp_socket_bind(const char* addr, int port);
SirinUdpPacket  sirin_udp_socket_recv_from(SirinUdpSocket* s);
void            sirin_udp_socket_send_to(SirinUdpSocket* s, const char* addr, int port, const char* data);
void            sirin_udp_socket_close(SirinUdpSocket* s);

/* ── init (called by emitted main when sirin.net is imported) ── */
void sirin_net_init(void);

#endif /* SIRIN_NET_H */
