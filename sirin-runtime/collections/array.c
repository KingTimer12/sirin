#include "array.h"
#include "../core/mem.h"

#include <stdio.h>
#include <stdlib.h>

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
