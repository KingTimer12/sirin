#include "sys.h"
#include "../sirin_net.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

void sirin_net_init(void) {
#ifdef _WIN32
    WSADATA data;
    if (WSAStartup(MAKEWORD(2, 2), &data) != 0) sirin_net_fail("WSAStartup() failed");
#endif
}

struct sockaddr_in sirin_net_addr(const char* addr, int port) {
    struct sockaddr_in sa;
    memset(&sa, 0, sizeof(sa));
    sa.sin_family      = AF_INET;
    sa.sin_port        = htons((unsigned short)port);
    sa.sin_addr.s_addr = inet_addr(addr);
    return sa;
}

void sirin_net_set_nonblocking(int fd) {
#ifdef _WIN32
    u_long on = 1;
    ioctlsocket(fd, FIONBIO, &on);
#else
    int flags = fcntl(fd, F_GETFL, 0);
    fcntl(fd, F_SETFL, flags | O_NONBLOCK);
#endif
}

int sirin_net_dup(int fd) {
#ifdef _WIN32
    WSAPROTOCOL_INFO info;
    if (WSADuplicateSocket((SOCKET)fd, GetCurrentProcessId(), &info) != 0) {
        sirin_net_fail("WSADuplicateSocket() failed in clone");
    }
    SOCKET copy = WSASocket(FROM_PROTOCOL_INFO, FROM_PROTOCOL_INFO, FROM_PROTOCOL_INFO,
                            &info, 0, WSA_FLAG_OVERLAPPED);
    if (copy == INVALID_SOCKET) sirin_net_fail("WSASocket() failed in clone");
    sirin_net_set_nonblocking((int)copy);
    return (int)copy;
#else
    int copy = dup(fd);
    if (copy < 0) sirin_net_fail("dup() failed in clone");
    return copy;
#endif
}

void sirin_net_fail(const char* what) {
    fprintf(stderr, "sirin_net: %s\n", what);
    exit(1);
}
