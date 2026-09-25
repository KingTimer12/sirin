#include "udp.h"
#include "sys.h"
#include "tcp.h"  /* SIRIN_TCP_READ_BUF */
#include "../core/mem.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

SirinUdpSocket sirin_udp_socket_bind(const char* addr, int port) {
    int fd = (int)socket(AF_INET, SOCK_DGRAM, 0);
    if (fd < 0) sirin_net_fail("socket() failed");
    struct sockaddr_in sa = sirin_net_addr(addr, port);
    if (bind(fd, (struct sockaddr*)&sa, sizeof(sa)) < 0) {
        fprintf(stderr, "sirin_net: udp bind() failed on %s:%d (is the port already in use?)\n", addr, port);
        exit(1);
    }
    return (SirinUdpSocket){ .fd = fd };
}

SirinUdpPacket sirin_udp_socket_recv_from(SirinUdpSocket* s) {
    char* buf = (char*)sirin_alloc(SIRIN_TCP_READ_BUF);
    struct sockaddr_in from;
    socklen_t fromlen = sizeof(from);
    int n = (int)recvfrom(s->fd, buf, SIRIN_TCP_READ_BUF - 1, 0,
                          (struct sockaddr*)&from, &fromlen);
    if (n < 0) sirin_net_fail("recvfrom() failed");
    buf[n] = '\0';

    SirinUdpPacket pkt;
    pkt.data = buf;
    pkt.port = ntohs(from.sin_port);
    const char* ip = inet_ntoa(from.sin_addr);
    snprintf(pkt.addr, sizeof(pkt.addr), "%s", ip ? ip : "");
    return pkt;
}

void sirin_udp_socket_send_to(SirinUdpSocket* s, const char* addr, int port, const char* data) {
    struct sockaddr_in sa = sirin_net_addr(addr, port);
    int n = (int)sendto(s->fd, data, (int)strlen(data), 0,
                        (struct sockaddr*)&sa, sizeof(sa));
    if (n < 0) sirin_net_fail("sendto() failed");
}

void sirin_udp_socket_close(SirinUdpSocket* s) {
    SIRIN_SOCK_CLOSE(s->fd);
    s->fd = -1;
}
