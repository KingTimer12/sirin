/* Set[T] for numbers and bools (compared with ==). Enabled by
   SIRIN_USE_SET_<T> in sirin_config.h. */
#ifndef SIRIN_COLLECTIONS_SET_H
#define SIRIN_COLLECTIONS_SET_H

#include "../sirin_config.h"

#include <stddef.h>
#include <stdint.h>

#define SIRIN_SET_DECL(TypeName, FuncName, CType)                              \
typedef struct {                                                                \
    CType*  ptr;                                                                \
    size_t  len;                                                                \
    size_t  cap;                                                                \
} SirinSet##TypeName;                                                           \
SirinSet##TypeName sirin_set_##FuncName##_new(void);                            \
void sirin_set_##FuncName##_insert(SirinSet##TypeName* s, CType value);         \
int  sirin_set_##FuncName##_contains(SirinSet##TypeName* s, CType value);       \
void sirin_set_##FuncName##_free(SirinSet##TypeName* s);

#ifdef SIRIN_USE_SET_INT
SIRIN_SET_DECL(Int,   int,   int64_t)
#endif
#ifdef SIRIN_USE_SET_U8
SIRIN_SET_DECL(U8,    u8,    uint8_t)
#endif
#ifdef SIRIN_USE_SET_U16
SIRIN_SET_DECL(U16,   u16,   uint16_t)
#endif
#ifdef SIRIN_USE_SET_U32
SIRIN_SET_DECL(U32,   u32,   uint32_t)
#endif
#ifdef SIRIN_USE_SET_U64
SIRIN_SET_DECL(U64,   u64,   uint64_t)
#endif
#ifdef SIRIN_USE_SET_I8
SIRIN_SET_DECL(I8,    i8,    int8_t)
#endif
#ifdef SIRIN_USE_SET_I16
SIRIN_SET_DECL(I16,   i16,   int16_t)
#endif
#ifdef SIRIN_USE_SET_I32
SIRIN_SET_DECL(I32,   i32,   int32_t)
#endif
#ifdef SIRIN_USE_SET_I64
SIRIN_SET_DECL(I64,   i64,   int64_t)
#endif
#ifdef SIRIN_USE_SET_FLOAT
SIRIN_SET_DECL(Float, float, double)
#endif
#ifdef SIRIN_USE_SET_BOOL
SIRIN_SET_DECL(Bool,  bool,  int)
#endif

#endif
