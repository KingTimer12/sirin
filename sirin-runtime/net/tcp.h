/* TcpListener and TcpStream. Waiting operations (accept, read, write)
   yield to other coroutines instead of blocking the program. */
#ifndef SIRIN_NET_TCP_H
#define SIRIN_NET_TCP_H

#define SIRIN_TCP_READ_BUF 4096

typedef struct { int fd; } SirinTcpListener;
typedef struct { int fd; } SirinTcpStream;

SirinTcpListener sirin_tcp_listener_bind(const char* addr, int port);
SirinTcpStream   sirin_tcp_listener_accept(SirinTcpListener* l);
void             sirin_tcp_listener_close(SirinTcpListener* l);
SirinTcpListener sirin_tcp_listener_clone(SirinTcpListener* l);

SirinTcpStream   sirin_tcp_stream_connect(const char* addr, int port);
const char*      sirin_tcp_stream_read(SirinTcpStream* s);   /* "" once the peer closes */
void             sirin_tcp_stream_write(SirinTcpStream* s, const char* data);
void             sirin_tcp_stream_close(SirinTcpStream* s);
SirinTcpStream   sirin_tcp_stream_clone(SirinTcpStream* s);

#endif
