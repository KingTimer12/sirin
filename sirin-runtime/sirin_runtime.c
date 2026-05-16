#include "sirin_runtime.h"

#include <stdlib.h>
#include <string.h>
#include <stdio.h>

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

SIRIN_VEC_IMPL(Int,   int,   int64_t)
SIRIN_VEC_IMPL(U8,    u8,    uint8_t)
SIRIN_VEC_IMPL(U16,   u16,   uint16_t)
SIRIN_VEC_IMPL(U32,   u32,   uint32_t)
SIRIN_VEC_IMPL(U64,   u64,   uint64_t)
SIRIN_VEC_IMPL(I8,    i8,    int8_t)
SIRIN_VEC_IMPL(I16,   i16,   int16_t)
SIRIN_VEC_IMPL(I32,   i32,   int32_t)
SIRIN_VEC_IMPL(I64,   i64,   int64_t)
SIRIN_VEC_IMPL(Float, float, double)
SIRIN_VEC_IMPL(Bool,  bool,  int)
SIRIN_VEC_IMPL(Str,   str,   SirinCStr)

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

SIRIN_ARRAY_IMPL(Int,   int,   int64_t)
SIRIN_ARRAY_IMPL(U8,    u8,    uint8_t)
SIRIN_ARRAY_IMPL(U16,   u16,   uint16_t)
SIRIN_ARRAY_IMPL(U32,   u32,   uint32_t)
SIRIN_ARRAY_IMPL(U64,   u64,   uint64_t)
SIRIN_ARRAY_IMPL(I8,    i8,    int8_t)
SIRIN_ARRAY_IMPL(I16,   i16,   int16_t)
SIRIN_ARRAY_IMPL(I32,   i32,   int32_t)
SIRIN_ARRAY_IMPL(I64,   i64,   int64_t)
SIRIN_ARRAY_IMPL(Float, float, double)
SIRIN_ARRAY_IMPL(Bool,  bool,  int)
SIRIN_ARRAY_IMPL(Str,   str,   SirinCStr)

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

SIRIN_SET_IMPL(Int,   int,   int64_t)
SIRIN_SET_IMPL(U8,    u8,    uint8_t)
SIRIN_SET_IMPL(U16,   u16,   uint16_t)
SIRIN_SET_IMPL(U32,   u32,   uint32_t)
SIRIN_SET_IMPL(U64,   u64,   uint64_t)
SIRIN_SET_IMPL(I8,    i8,    int8_t)
SIRIN_SET_IMPL(I16,   i16,   int16_t)
SIRIN_SET_IMPL(I32,   i32,   int32_t)
SIRIN_SET_IMPL(I64,   i64,   int64_t)
SIRIN_SET_IMPL(Float, float, double)
SIRIN_SET_IMPL(Bool,  bool,  int)

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
void sirin_map_str_##FuncName##_free(SirinMapStr##TypeName* m) {                  \
    for (size_t i = 0; i < m->len; i++) free(m->keys[i]);                         \
    free(m->keys); free(m->vals);                                                  \
    m->keys = NULL; m->vals = NULL; m->len = 0; m->cap = 0;                       \
}

SIRIN_MAP_STR_IMPL(Int,   int,   int64_t)
SIRIN_MAP_STR_IMPL(Str,   str,   SirinCStr)
SIRIN_MAP_STR_IMPL(Float, float, double)
