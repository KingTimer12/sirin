#include "set.h"
#include "../core/mem.h"

#include <stdlib.h>

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
    if (sirin_set_##FuncName##_contains(s, value)) return;                        \
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
