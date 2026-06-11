#include "sirin_runtime.h"

#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <ctype.h>

#ifndef _WIN32
    #include <unistd.h>
    #include <fcntl.h>
    #include <errno.h>
#endif

/* Provided by sirin_async.c (always linked). Declared here to avoid header
   coupling — readln cooperates with the event loop when run in a coroutine. */
extern int  sirin_in_coroutine(void);
extern void sirin_yield(void);

/* ── internal ─────────────────────────────────────────────────────────────── */

static void* sirin_alloc(size_t n) {
    void* p = malloc(n);
    if (!p) {
        fprintf(stderr, "sirin runtime: out of memory (malloc %lu bytes)\n", (unsigned long)n);
        exit(1);
    }
    return p;
}

static void* sirin_realloc(void* p, size_t n) {
    void* q = realloc(p, n);
    if (!q) {
        fprintf(stderr, "sirin runtime: out of memory (realloc %lu bytes)\n", (unsigned long)n);
        exit(1);
    }
    return q;
}

/* Heap-duplicate n bytes of s as a NUL-terminated string (caller owns; leaked
   like other Sirin strings — the language has no GC yet). */
static char* sirin__dup(const char* s, size_t n) {
    char* buf = (char*)sirin_alloc(n + 1);
    memcpy(buf, s, n);
    buf[n] = '\0';
    return buf;
}

/* ── SirinStr ─────────────────────────────────────────────────────────────── */

SirinStr sirin_str_new(const char* literal) {
    size_t len = strlen(literal);
    char* buf = (char*)sirin_alloc(len);
    memcpy(buf, literal, len);
    return (SirinStr){ .ptr = buf, .len = len };
}

SirinStr sirin_str_copy(SirinStr s) {
    char* buf = (char*)sirin_alloc(s.len);
    memcpy(buf, s.ptr, s.len);
    return (SirinStr){ .ptr = buf, .len = s.len };
}

void sirin_str_free(SirinStr s) { free(s.ptr); }

int sirin_str_eq(SirinStr a, SirinStr b) {
    return a.len == b.len && memcmp(a.ptr, b.ptr, a.len) == 0;
}

/* ── str ops on plain const char* ─────────────────────────────────────────── */

const char* sirin_str_clone(const char* s) { return sirin__dup(s, strlen(s)); }

void sirin_cstr_free(const char* s) { free((void*)s); }

/* Concatenate two strings into a fresh heap buffer (backs `str + str`). */
const char* sirin_str_concat(const char* a, const char* b) {
    size_t la = strlen(a), lb = strlen(b);
    char* out = (char*)sirin_alloc(la + lb + 1);
    memcpy(out, a, la);
    memcpy(out + la, b, lb);
    out[la + lb] = '\0';
    return out;
}

/* Decimal rendering of an integer into a fresh heap buffer (backs `int.to_str()`). */
const char* sirin_int_to_str(int64_t n) {
    char buf[32];
    int len = snprintf(buf, sizeof(buf), "%lld", (long long)n);
    char* out = (char*)sirin_alloc((size_t)len + 1);
    memcpy(out, buf, (size_t)len);
    out[len] = '\0';
    return out;
}

int64_t sirin_str_len(const char* s) { return (int64_t)strlen(s); }

const char* sirin_str_char_at(const char* s, int64_t i) {
    int64_t n = (int64_t)strlen(s);
    if (i < 0 || i >= n) return "";
    return sirin__dup(s + i, 1);
}

const char* sirin_str_slice(const char* s, int64_t start, int64_t end) {
    int64_t n = (int64_t)strlen(s);
    if (start < 0) start += n;          /* negative = from end */
    if (end   < 0) end   += n;
    if (start < 0) start = 0;
    if (end   > n) end   = n;
    if (start >= end) return "";
    return sirin__dup(s + start, (size_t)(end - start));
}

int64_t sirin_str_index_of(const char* s, const char* sub) {
    const char* p = strstr(s, sub);
    return p ? (int64_t)(p - s) : -1;
}

int sirin_str_contains(const char* s, const char* sub) {
    return strstr(s, sub) != NULL;
}

int sirin_str_starts_with(const char* s, const char* pre) {
    return strncmp(s, pre, strlen(pre)) == 0;
}

int sirin_str_ends_with(const char* s, const char* suf) {
    size_t ls = strlen(s), lf = strlen(suf);
    if (lf > ls) return 0;
    return memcmp(s + ls - lf, suf, lf) == 0;
}

const char* sirin_str_trim(const char* s) {
    while (*s && isspace((unsigned char)*s)) s++;
    const char* end = s + strlen(s);
    while (end > s && isspace((unsigned char)end[-1])) end--;
    return sirin__dup(s, (size_t)(end - s));
}

int64_t sirin_str_to_int(const char* s)   { return (int64_t)strtoll(s, NULL, 10); }
double  sirin_str_to_float(const char* s) { return strtod(s, NULL); }

const char* sirin_str_to_upper(const char* s) {
    size_t n = strlen(s);
    char* buf = (char*)sirin_alloc(n + 1);
    for (size_t i = 0; i < n; i++) buf[i] = (char)toupper((unsigned char)s[i]);
    buf[n] = '\0';
    return buf;
}

const char* sirin_str_to_lower(const char* s) {
    size_t n = strlen(s);
    char* buf = (char*)sirin_alloc(n + 1);
    for (size_t i = 0; i < n; i++) buf[i] = (char)tolower((unsigned char)s[i]);
    buf[n] = '\0';
    return buf;
}

const char* sirin_str_replace(const char* s, const char* from, const char* to) {
    size_t lf = strlen(from);
    if (lf == 0) return sirin__dup(s, strlen(s));
    size_t lt = strlen(to), ls = strlen(s), count = 0;
    for (const char* p = s; (p = strstr(p, from)); p += lf) count++;
    char* buf = (char*)sirin_alloc(ls + count * (lt > lf ? lt - lf : 0) + 1);
    char* out = buf;
    const char* p = s;
    for (;;) {
        const char* hit = strstr(p, from);
        if (!hit) { strcpy(out, p); break; }
        size_t chunk = (size_t)(hit - p);
        memcpy(out, p, chunk); out += chunk;
        memcpy(out, to, lt);   out += lt;
        p = hit + lf;
    }
    return buf;
}

/* ── minimal JSON field extraction ────────────────────────────────────────── */

/* Locate the value position for "key" in a flat JSON object. Returns a pointer
   to the first non-space char after the colon, or NULL if the key is absent. */
static const char* sirin__json_find(const char* json, const char* key) {
    size_t lk = strlen(key);
    const char* p = json;
    while ((p = strchr(p, '"'))) {
        const char* kstart = p + 1;
        const char* kend = strchr(kstart, '"');
        if (!kend) return NULL;
        size_t klen = (size_t)(kend - kstart);
        const char* after = kend + 1;
        while (*after && isspace((unsigned char)*after)) after++;
        if (*after == ':' && klen == lk && memcmp(kstart, key, lk) == 0) {
            after++;
            while (*after && isspace((unsigned char)*after)) after++;
            return after;
        }
        p = kend + 1;
    }
    return NULL;
}

const char* sirin_json_get_str(const char* json, const char* key) {
    const char* v = sirin__json_find(json, key);
    if (!v || *v != '"') return "";
    const char* start = v + 1;
    const char* p = start;
    while (*p && *p != '"') { if (*p == '\\' && p[1]) p++; p++; }
    return sirin__dup(start, (size_t)(p - start));
}

int64_t sirin_json_get_int(const char* json, const char* key) {
    const char* v = sirin__json_find(json, key);
    return v ? (int64_t)strtoll(v, NULL, 10) : 0;
}

double sirin_json_get_float(const char* json, const char* key) {
    const char* v = sirin__json_find(json, key);
    return v ? strtod(v, NULL) : 0.0;
}

int sirin_json_get_bool(const char* json, const char* key) {
    const char* v = sirin__json_find(json, key);
    return v && strncmp(v, "true", 4) == 0;
}

/* ── Vec impl macro ───────────────────────────────────────────────────────── */

#define SIRIN_VEC_IMPL(TypeName, FuncName, CType)                              \
SirinVec##TypeName sirin_vec_##FuncName##_new(size_t initial_cap) {            \
    size_t cap = initial_cap > 0 ? initial_cap : 4;                            \
    CType* buf = (CType*)sirin_alloc(cap * sizeof(CType));                     \
    return (SirinVec##TypeName){ .ptr = buf, .len = 0, .cap = cap };           \
}                                                                               \
void sirin_vec_##FuncName##_push(SirinVec##TypeName* v, CType value) {         \
    if (v->len == v->cap) {                                                     \
        v->cap *= 2;                                                            \
        v->ptr = (CType*)sirin_realloc(v->ptr, v->cap * sizeof(CType));        \
    }                                                                           \
    v->ptr[v->len++] = value;                                                   \
}                                                                               \
CType sirin_vec_##FuncName##_get(SirinVec##TypeName* v, size_t index) {        \
    if (index >= v->len) {                                                      \
        fprintf(stderr,                                                         \
            "sirin runtime: vec index out of bounds (index=%lu, len=%lu)\n",   \
            (unsigned long)index, (unsigned long)v->len);                       \
        exit(1);                                                                \
    }                                                                           \
    return v->ptr[index];                                                       \
}                                                                               \
void sirin_vec_##FuncName##_free(SirinVec##TypeName* v) {                      \
    free(v->ptr); v->ptr = NULL; v->len = 0; v->cap = 0;                       \
}

#ifdef SIRIN_USE_VEC_INT
SIRIN_VEC_IMPL(Int,   int,   int64_t)
#endif
#ifdef SIRIN_USE_VEC_U8
SIRIN_VEC_IMPL(U8,    u8,    uint8_t)
#endif
#ifdef SIRIN_USE_VEC_U16
SIRIN_VEC_IMPL(U16,   u16,   uint16_t)
#endif
#ifdef SIRIN_USE_VEC_U32
SIRIN_VEC_IMPL(U32,   u32,   uint32_t)
#endif
#ifdef SIRIN_USE_VEC_U64
SIRIN_VEC_IMPL(U64,   u64,   uint64_t)
#endif
#ifdef SIRIN_USE_VEC_I8
SIRIN_VEC_IMPL(I8,    i8,    int8_t)
#endif
#ifdef SIRIN_USE_VEC_I16
SIRIN_VEC_IMPL(I16,   i16,   int16_t)
#endif
#ifdef SIRIN_USE_VEC_I32
SIRIN_VEC_IMPL(I32,   i32,   int32_t)
#endif
#ifdef SIRIN_USE_VEC_I64
SIRIN_VEC_IMPL(I64,   i64,   int64_t)
#endif
#ifdef SIRIN_USE_VEC_FLOAT
SIRIN_VEC_IMPL(Float, float, double)
#endif
#ifdef SIRIN_USE_VEC_BOOL
SIRIN_VEC_IMPL(Bool,  bool,  int)
#endif
#ifdef SIRIN_USE_VEC_STR
SIRIN_VEC_IMPL(Str,   str,   SirinCStr)
SirinVecStr sirin_str_split(const char* s, const char* sep) {
    SirinVecStr v = sirin_vec_str_new(4);
    size_t lsep = strlen(sep);
    if (lsep == 0) { sirin_vec_str_push(&v, sirin__dup(s, strlen(s))); return v; }
    const char* p = s;
    for (;;) {
        const char* hit = strstr(p, sep);
        if (!hit) { sirin_vec_str_push(&v, sirin__dup(p, strlen(p))); break; }
        sirin_vec_str_push(&v, sirin__dup(p, (size_t)(hit - p)));
        p = hit + lsep;
    }
    return v;
}
#endif

/* ── Array impl macro ─────────────────────────────────────────────────────── */

#define SIRIN_ARRAY_IMPL(TypeName, FuncName, CType)                               \
SirinArray##TypeName sirin_array_##FuncName##_new(size_t initial_cap) {           \
    size_t cap = initial_cap > 0 ? initial_cap : 4;                               \
    CType* buf = (CType*)sirin_alloc(cap * sizeof(CType));                        \
    return (SirinArray##TypeName){ .ptr = buf, .len = 0, .cap = cap };            \
}                                                                                  \
void sirin_array_##FuncName##_push(SirinArray##TypeName* v, CType value) {        \
    if (v->len == v->cap) {                                                        \
        v->cap *= 2;                                                               \
        v->ptr = (CType*)sirin_realloc(v->ptr, v->cap * sizeof(CType));           \
    }                                                                              \
    v->ptr[v->len++] = value;                                                      \
}                                                                                  \
CType sirin_array_##FuncName##_get(SirinArray##TypeName* v, size_t index) {       \
    if (index >= v->len) {                                                         \
        fprintf(stderr,                                                            \
            "sirin runtime: array index out of bounds (index=%lu, len=%lu)\n",    \
            (unsigned long)index, (unsigned long)v->len);                          \
        exit(1);                                                                   \
    }                                                                              \
    return v->ptr[index];                                                          \
}                                                                                  \
void sirin_array_##FuncName##_free(SirinArray##TypeName* v) {                     \
    free(v->ptr); v->ptr = NULL; v->len = 0; v->cap = 0;                          \
}

#ifdef SIRIN_USE_ARRAY_INT
SIRIN_ARRAY_IMPL(Int,   int,   int64_t)
#endif
#ifdef SIRIN_USE_ARRAY_U8
SIRIN_ARRAY_IMPL(U8,    u8,    uint8_t)
#endif
#ifdef SIRIN_USE_ARRAY_U16
SIRIN_ARRAY_IMPL(U16,   u16,   uint16_t)
#endif
#ifdef SIRIN_USE_ARRAY_U32
SIRIN_ARRAY_IMPL(U32,   u32,   uint32_t)
#endif
#ifdef SIRIN_USE_ARRAY_U64
SIRIN_ARRAY_IMPL(U64,   u64,   uint64_t)
#endif
#ifdef SIRIN_USE_ARRAY_I8
SIRIN_ARRAY_IMPL(I8,    i8,    int8_t)
#endif
#ifdef SIRIN_USE_ARRAY_I16
SIRIN_ARRAY_IMPL(I16,   i16,   int16_t)
#endif
#ifdef SIRIN_USE_ARRAY_I32
SIRIN_ARRAY_IMPL(I32,   i32,   int32_t)
#endif
#ifdef SIRIN_USE_ARRAY_I64
SIRIN_ARRAY_IMPL(I64,   i64,   int64_t)
#endif
#ifdef SIRIN_USE_ARRAY_FLOAT
SIRIN_ARRAY_IMPL(Float, float, double)
#endif
#ifdef SIRIN_USE_ARRAY_BOOL
SIRIN_ARRAY_IMPL(Bool,  bool,  int)
#endif

/* ── Set impl macro (== comparison; inlines contains inside insert) ─────────  */

#define SIRIN_SET_IMPL(TypeName, FuncName, CType)                                 \
SirinSet##TypeName sirin_set_##FuncName##_new(void) {                             \
    size_t cap = 4;                                                               \
    CType* ptr = (CType*)sirin_alloc(cap * sizeof(CType));                        \
    return (SirinSet##TypeName){ .ptr = ptr, .len = 0, .cap = cap };              \
}                                                                                  \
int sirin_set_##FuncName##_contains(SirinSet##TypeName* s, CType value) {         \
    for (size_t i = 0; i < s->len; i++) if (s->ptr[i] == value) return 1;        \
    return 0;                                                                      \
}                                                                                  \
void sirin_set_##FuncName##_insert(SirinSet##TypeName* s, CType value) {          \
    for (size_t i = 0; i < s->len; i++) if (s->ptr[i] == value) return;          \
    if (s->len == s->cap) {                                                        \
        s->cap *= 2;                                                               \
        s->ptr = (CType*)sirin_realloc(s->ptr, s->cap * sizeof(CType));           \
    }                                                                              \
    s->ptr[s->len++] = value;                                                      \
}                                                                                  \
void sirin_set_##FuncName##_free(SirinSet##TypeName* s) {                         \
    free(s->ptr); s->ptr = NULL; s->len = 0; s->cap = 0;                          \
}

#ifdef SIRIN_USE_SET_INT
SIRIN_SET_IMPL(Int,   int,   int64_t)
#endif
#ifdef SIRIN_USE_SET_U8
SIRIN_SET_IMPL(U8,    u8,    uint8_t)
#endif
#ifdef SIRIN_USE_SET_U16
SIRIN_SET_IMPL(U16,   u16,   uint16_t)
#endif
#ifdef SIRIN_USE_SET_U32
SIRIN_SET_IMPL(U32,   u32,   uint32_t)
#endif
#ifdef SIRIN_USE_SET_U64
SIRIN_SET_IMPL(U64,   u64,   uint64_t)
#endif
#ifdef SIRIN_USE_SET_I8
SIRIN_SET_IMPL(I8,    i8,    int8_t)
#endif
#ifdef SIRIN_USE_SET_I16
SIRIN_SET_IMPL(I16,   i16,   int16_t)
#endif
#ifdef SIRIN_USE_SET_I32
SIRIN_SET_IMPL(I32,   i32,   int32_t)
#endif
#ifdef SIRIN_USE_SET_I64
SIRIN_SET_IMPL(I64,   i64,   int64_t)
#endif
#ifdef SIRIN_USE_SET_FLOAT
SIRIN_SET_IMPL(Float, float, double)
#endif
#ifdef SIRIN_USE_SET_BOOL
SIRIN_SET_IMPL(Bool,  bool,  int)
#endif

/* ── Map[str, T] ──────────────────────────────────────────────────────────── */

#define SIRIN_MAP_STR_IMPL(TypeName, FuncName, ValCType)                          \
SirinMapStr##TypeName sirin_map_str_##FuncName##_new(void) {                      \
    size_t cap = 4;                                                               \
    char**     keys = (char**)sirin_alloc(cap * sizeof(char*));                   \
    ValCType*  vals = (ValCType*)sirin_alloc(cap * sizeof(ValCType));             \
    return (SirinMapStr##TypeName){ .keys = keys, .vals = vals, .len = 0, .cap = cap }; \
}                                                                                  \
void sirin_map_str_##FuncName##_insert(SirinMapStr##TypeName* m,                  \
                                       const char* key, ValCType value) {          \
    for (size_t i = 0; i < m->len; i++) {                                         \
        if (strcmp(m->keys[i], key) == 0) { m->vals[i] = value; return; }         \
    }                                                                              \
    if (m->len == m->cap) {                                                        \
        m->cap *= 2;                                                               \
        m->keys = (char**)sirin_realloc(m->keys, m->cap * sizeof(char*));         \
        m->vals = (ValCType*)sirin_realloc(m->vals, m->cap * sizeof(ValCType));   \
    }                                                                              \
    size_t klen = strlen(key);                                                     \
    char* k = (char*)sirin_alloc(klen + 1);                                       \
    memcpy(k, key, klen); k[klen] = '\0';                                         \
    m->keys[m->len] = k;                                                           \
    m->vals[m->len] = value;                                                       \
    m->len++;                                                                      \
}                                                                                  \
ValCType sirin_map_str_##FuncName##_get(SirinMapStr##TypeName* m, const char* key) { \
    for (size_t i = 0; i < m->len; i++) {                                         \
        if (strcmp(m->keys[i], key) == 0) return m->vals[i];                      \
    }                                                                              \
    fprintf(stderr, "sirin runtime: map key not found: \"%s\"\n", key);           \
    exit(1);                                                                       \
}                                                                                  \
ValCType* sirin_map_str_##FuncName##_get_opt(SirinMapStr##TypeName* m,            \
                                             const char* key) {                    \
    for (size_t i = 0; i < m->len; i++) {                                         \
        if (strcmp(m->keys[i], key) == 0) return &m->vals[i];                     \
    }                                                                              \
    return NULL;                                                                   \
}                                                                                  \
int64_t sirin_map_str_##FuncName##_len(SirinMapStr##TypeName* m) {                \
    return (int64_t)m->len;                                                        \
}                                                                                  \
const char** sirin_map_str_##FuncName##_key_at(SirinMapStr##TypeName* m,          \
                                               int64_t i) {                        \
    if (i < 0 || (size_t)i >= m->len) return NULL;                                \
    return (const char**)&m->keys[i];                                             \
}                                                                                  \
void sirin_map_str_##FuncName##_free(SirinMapStr##TypeName* m) {                  \
    for (size_t i = 0; i < m->len; i++) free(m->keys[i]);                         \
    free(m->keys); free(m->vals);                                                  \
    m->keys = NULL; m->vals = NULL; m->len = 0; m->cap = 0;                       \
}

#ifdef SIRIN_USE_MAP_STR_INT
SIRIN_MAP_STR_IMPL(Int,   int,   int64_t)
#endif
#ifdef SIRIN_USE_MAP_STR_STR
SIRIN_MAP_STR_IMPL(Str,   str,   SirinCStr)
#endif
#ifdef SIRIN_USE_MAP_STR_FLOAT
SIRIN_MAP_STR_IMPL(Float, float, double)
#endif

const char* sirin_readln(void) {
    static char buf[1024];
#ifndef _WIN32
    /* In a coroutine: read stdin non-blocking and yield so other coroutines
       (e.g. a socket read loop) keep running while waiting for user input. */
    if (sirin_in_coroutine()) {
        static int initialized = 0;
        if (!initialized) {
            int fl = fcntl(0, F_GETFL, 0);
            fcntl(0, F_SETFL, fl | O_NONBLOCK);
            initialized = 1;
        }
        size_t len = 0;
        for (;;) {
            char c;
            ssize_t r = read(0, &c, 1);
            if (r > 0) {
                if (c == '\n') break;
                if (len < sizeof(buf) - 1) buf[len++] = c;
            } else if (r == 0) {
                if (len == 0) return "";  /* EOF */
                break;
            } else {
                if (errno == EAGAIN || errno == EWOULDBLOCK) { sirin_yield(); continue; }
                break;
            }
        }
        buf[len] = '\0';
        return buf;
    }
#endif
    if (!fgets(buf, sizeof(buf), stdin)) return "";
    size_t len = strlen(buf);
    if (len > 0 && buf[len-1] == '\n') buf[--len] = '\0';
    return buf;
}
