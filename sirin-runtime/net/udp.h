/* UdpSocket: connectionless datagrams. */
#ifndef SIRIN_NET_UDP_H
#define SIRIN_NET_UDP_H

typedef struct { int fd; } SirinUdpSocket;

typedef struct {
    const char* data;
    char        addr[64];
    int         port;
} SirinUdpPacket;

SirinUdpSocket sirin_udp_socket_bind(const char* addr, int port);
SirinUdpPacket sirin_udp_socket_recv_from(SirinUdpSocket* s);
void           sirin_udp_socket_send_to(SirinUdpSocket* s, const char* addr, int port, const char* data);
void           sirin_udp_socket_close(SirinUdpSocket* s);

#endif
