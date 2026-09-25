/* Internal: one socket API over Winsock and BSD sockets, so tcp.c and udp.c
   contain no platform #ifdefs. Not included by generated programs. */
#ifndef SIRIN_NET_SYS_H
#define SIRIN_NET_SYS_H

#ifdef _WIN32
    /* inet_addr/inet_ntoa are IPv4-only, which is all Sirin supports for now. */
    #ifndef _WINSOCK_DEPRECATED_NO_WARNINGS
    #define _WINSOCK_DEPRECATED_NO_WARNINGS
    #endif
    #include <winsock2.h>
    #include <ws2tcpip.h>
    typedef int socklen_t;
    #define SIRIN_SOCK_CLOSE(fd)   closesocket(fd)
    #define SIRIN_WOULD_BLOCK()    (WSAGetLastError() == WSAEWOULDBLOCK)
#else
    #include <arpa/inet.h>
    #include <errno.h>
    #include <fcntl.h>
    #include <netinet/in.h>
    #include <sys/socket.h>
    #include <unistd.h>
    #define SIRIN_SOCK_CLOSE(fd)   close(fd)
    #define SIRIN_WOULD_BLOCK()    (errno == EAGAIN || errno == EWOULDBLOCK)
#endif

/* IPv4 address for `addr:port`. */
struct sockaddr_in sirin_net_addr(const char* addr, int port);

/* Switch a socket to non-blocking mode, so waits can yield to other coroutines. */
void sirin_net_set_nonblocking(int fd);

/* A second, independent handle to the same socket (backs `::clone`). */
int sirin_net_dup(int fd);

/* Print "sirin_net: <what>" to stderr and exit. */
void sirin_net_fail(const char* what);

#endif
