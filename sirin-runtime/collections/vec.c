#include "vec.h"
#include "../core/mem.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

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
    if (lsep == 0) { sirin_vec_str_push(&v, sirin_dup(s, strlen(s))); return v; }
    const char* p = s;
    for (;;) {
        const char* hit = strstr(p, sep);
        if (!hit) { sirin_vec_str_push(&v, sirin_dup(p, strlen(p))); break; }
        sirin_vec_str_push(&v, sirin_dup(p, (size_t)(hit - p)));
        p = hit + lsep;
    }
    return v;
}
#endif
