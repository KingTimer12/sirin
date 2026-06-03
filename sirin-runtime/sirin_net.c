#ifdef _WIN32
    #include <winsock2.h>
    #include <ws2tcpip.h>
    #pragma comment(lib, "ws2_32.lib")
    #define SIRIN_SOCK_INIT() do { WSADATA _w; WSAStartup(MAKEWORD(2,2), &_w); } while(0)
    #define SIRIN_SOCK_CLOSE(fd) closesocket(fd)
    #define SIRIN_SET_NONBLOCK(fd) do { u_long _m = 1; ioctlsocket(fd, FIONBIO, &_m); } while(0)
    #define SIRIN_WOULD_BLOCK() (WSAGetLastError() == WSAEWOULDBLOCK)
    typedef int socklen_t;
#else
    #include <sys/socket.h>
    #include <netinet/in.h>
    #include <arpa/inet.h>
    #include <unistd.h>
    #include <fcntl.h>
    #include <errno.h>
    #define SIRIN_SOCK_INIT() do {} while(0)
    #define SIRIN_SOCK_CLOSE(fd) close(fd)
    #define SIRIN_SET_NONBLOCK(fd) do { int _f = fcntl(fd, F_GETFL, 0); fcntl(fd, F_SETFL, _f | O_NONBLOCK); } while(0)
    #define SIRIN_WOULD_BLOCK() (errno == EAGAIN || errno == EWOULDBLOCK)
#endif

#include <string.h>
#include <stdio.h>
#include <stdlib.h>
#include "sirin_net.h"
#include "sirin_async.h"

void sirin_net_init(void) {
    SIRIN_SOCK_INIT();
}

/* ── helpers ── */

static struct sockaddr_in make_addr(const char* addr, int port) {
    struct sockaddr_in sa;
    memset(&sa, 0, sizeof(sa));
    sa.sin_family      = AF_INET;
    sa.sin_port        = htons((unsigned short)port);
    sa.sin_addr.s_addr = inet_addr(addr);
    return sa;
}

/* ── TcpListener ── */

SirinTcpListener sirin_tcp_listener_bind(const char* addr, int port) {
    int fd = (int)socket(AF_INET, SOCK_STREAM, 0);
    if (fd < 0) { fprintf(stderr, "sirin_net: socket() failed\n"); exit(1); }

    int yes = 1;
    setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, (const char*)&yes, sizeof(yes));

    struct sockaddr_in sa = make_addr(addr, port);
    if (bind(fd, (struct sockaddr*)&sa, sizeof(sa)) < 0) {
        fprintf(stderr, "sirin_net: bind() failed on %s:%d\n", addr, port);
        exit(1);
    }
    if (listen(fd, 128) < 0) {
        fprintf(stderr, "sirin_net: listen() failed\n");
        exit(1);
    }
    SIRIN_SET_NONBLOCK(fd);
    SirinTcpListener l;
    l.fd = fd;
    return l;
}

SirinTcpStream sirin_tcp_listener_accept(SirinTcpListener* l) {
    struct sockaddr_in client;
    socklen_t len = sizeof(client);
    int cfd;
    for (;;) {
        len = sizeof(client);
        cfd = (int)accept(l->fd, (struct sockaddr*)&client, &len);
        if (cfd >= 0) break;
        if (SIRIN_WOULD_BLOCK()) { sirin_yield(); continue; }
        fprintf(stderr, "sirin_net: accept() failed\n"); exit(1);
    }
    SIRIN_SET_NONBLOCK(cfd);
    SirinTcpStream s;
    s.fd = cfd;
    return s;
}

void sirin_tcp_listener_close(SirinTcpListener* l) {
    SIRIN_SOCK_CLOSE(l->fd);
    l->fd = -1;
}

/* Deep clone: duplicate the OS handle so each copy owns an independent fd. */
SirinTcpListener sirin_tcp_listener_clone(SirinTcpListener* l) {
    SirinTcpListener copy;
#ifdef _WIN32
    WSAPROTOCOL_INFO info;
    WSADuplicateSocket((SOCKET)l->fd, GetCurrentProcessId(), &info);
    copy.fd = (int)WSASocket(AF_INET, SOCK_STREAM, 0, &info, 0, WSA_FLAG_OVERLAPPED);
#else
    copy.fd = dup(l->fd);
    if (copy.fd < 0) {
        fprintf(stderr, "sirin_net: dup() failed in tcp_listener_clone\n");
        exit(1);
    }
#endif
    return copy;
}

/* ── TcpStream ── */

SirinTcpStream sirin_tcp_stream_connect(const char* addr, int port) {
    int fd = (int)socket(AF_INET, SOCK_STREAM, 0);
    if (fd < 0) { fprintf(stderr, "sirin_net: socket() failed\n"); exit(1); }
    struct sockaddr_in sa = make_addr(addr, port);
    if (connect(fd, (struct sockaddr*)&sa, sizeof(sa)) < 0) {
        fprintf(stderr, "sirin_net: connect() failed to %s:%d\n", addr, port);
        exit(1);
    }
    SIRIN_SET_NONBLOCK(fd);
    SirinTcpStream s;
    s.fd = fd;
    return s;
}

const char* sirin_tcp_stream_read(SirinTcpStream* s) {
    char* buf = (char*)malloc(SIRIN_TCP_READ_BUF);
    if (!buf) { fprintf(stderr, "sirin_net: malloc failed\n"); exit(1); }
    int n;
    for (;;) {
        n = (int)recv(s->fd, buf, SIRIN_TCP_READ_BUF - 1, 0);
        if (n >= 0) break;
        if (SIRIN_WOULD_BLOCK()) { sirin_yield(); continue; }
        fprintf(stderr, "sirin_net: recv() failed\n"); exit(1);
    }
    buf[n] = '\0';  /* n == 0 → peer closed; returns "" */
    return buf;
}

void sirin_tcp_stream_write(SirinTcpStream* s, const char* data) {
    size_t total = strlen(data);
    size_t sent  = 0;
    while (sent < total) {
        int n = (int)send(s->fd, data + sent, (int)(total - sent), 0);
        if (n < 0) {
            if (SIRIN_WOULD_BLOCK()) { sirin_yield(); continue; }
            return;  /* broken pipe / closed peer — drop, non-fatal */
        }
        if (n == 0) return;
        sent += (size_t)n;
    }
}

void sirin_tcp_stream_close(SirinTcpStream* s) {
    SIRIN_SOCK_CLOSE(s->fd);
    s->fd = -1;
}

/* Deep clone: duplicate the OS handle so each copy owns an independent fd. */
SirinTcpStream sirin_tcp_stream_clone(SirinTcpStream* s) {
    SirinTcpStream copy;
#ifdef _WIN32
    WSAPROTOCOL_INFO info;
    WSADuplicateSocket((SOCKET)s->fd, GetCurrentProcessId(), &info);
    copy.fd = (int)WSASocket(AF_INET, SOCK_STREAM, 0, &info, 0, WSA_FLAG_OVERLAPPED);
#else
    copy.fd = dup(s->fd);
    if (copy.fd < 0) {
        fprintf(stderr, "sirin_net: dup() failed in tcp_stream_clone\n");
        exit(1);
    }
#endif
    return copy;
}

/* ── UdpSocket ── */

SirinUdpSocket sirin_udp_socket_bind(const char* addr, int port) {
    int fd = (int)socket(AF_INET, SOCK_DGRAM, 0);
    if (fd < 0) { fprintf(stderr, "sirin_net: socket() failed\n"); exit(1); }
    struct sockaddr_in sa = make_addr(addr, port);
    if (bind(fd, (struct sockaddr*)&sa, sizeof(sa)) < 0) {
        fprintf(stderr, "sirin_net: udp bind() failed on %s:%d\n", addr, port);
        exit(1);
    }
    SirinUdpSocket sock;
    sock.fd = fd;
    return sock;
}

SirinUdpPacket sirin_udp_socket_recv_from(SirinUdpSocket* s) {
    char* buf = (char*)malloc(SIRIN_TCP_READ_BUF);
    if (!buf) { fprintf(stderr, "sirin_net: malloc failed\n"); exit(1); }
    struct sockaddr_in from;
    socklen_t fromlen = sizeof(from);
    int n = (int)recvfrom(s->fd, buf, SIRIN_TCP_READ_BUF - 1, 0,
                          (struct sockaddr*)&from, &fromlen);
    if (n < 0) { fprintf(stderr, "sirin_net: recvfrom() failed\n"); exit(1); }
    buf[n] = '\0';

    SirinUdpPacket pkt;
    pkt.data = buf;
    pkt.port = ntohs(from.sin_port);
    const char* ip = inet_ntoa(from.sin_addr);
    strncpy(pkt.addr, ip ? ip : "", 63);
    pkt.addr[63] = '\0';
    return pkt;
}

void sirin_udp_socket_send_to(SirinUdpSocket* s, const char* addr, int port, const char* data) {
    struct sockaddr_in sa = make_addr(addr, port);
    size_t total = strlen(data);
    int n = (int)sendto(s->fd, data, (int)total, 0,
                        (struct sockaddr*)&sa, sizeof(sa));
    if (n < 0) { fprintf(stderr, "sirin_net: sendto() failed\n"); exit(1); }
}

void sirin_udp_socket_close(SirinUdpSocket* s) {
    SIRIN_SOCK_CLOSE(s->fd);
    s->fd = -1;
}
