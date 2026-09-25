/* Array[T]: array literal storage. Same layout and API as Vec[T], kept as a
   distinct type so the two can't be mixed up. Enabled by SIRIN_USE_ARRAY_<T>. */
#ifndef SIRIN_COLLECTIONS_ARRAY_H
#define SIRIN_COLLECTIONS_ARRAY_H

#include "../sirin_config.h"

#include <stddef.h>
#include <stdint.h>

#define SIRIN_ARRAY_DECL(TypeName, FuncName, CType)                            \
typedef struct {                                                                \
    CType*  ptr;                                                                \
    size_t  len;                                                                \
    size_t  cap;                                                                \
} SirinArray##TypeName;                                                         \
SirinArray##TypeName sirin_array_##FuncName##_new(size_t initial_cap);          \
void  sirin_array_##FuncName##_push(SirinArray##TypeName* v, CType value);      \
CType sirin_array_##FuncName##_get(SirinArray##TypeName* v, size_t index);      \
void  sirin_array_##FuncName##_free(SirinArray##TypeName* v);

#ifdef SIRIN_USE_ARRAY_INT
SIRIN_ARRAY_DECL(Int,   int,   int64_t)
#endif
#ifdef SIRIN_USE_ARRAY_U8
SIRIN_ARRAY_DECL(U8,    u8,    uint8_t)
#endif
#ifdef SIRIN_USE_ARRAY_U16
SIRIN_ARRAY_DECL(U16,   u16,   uint16_t)
#endif
#ifdef SIRIN_USE_ARRAY_U32
SIRIN_ARRAY_DECL(U32,   u32,   uint32_t)
#endif
#ifdef SIRIN_USE_ARRAY_U64
SIRIN_ARRAY_DECL(U64,   u64,   uint64_t)
#endif
#ifdef SIRIN_USE_ARRAY_I8
SIRIN_ARRAY_DECL(I8,    i8,    int8_t)
#endif
#ifdef SIRIN_USE_ARRAY_I16
SIRIN_ARRAY_DECL(I16,   i16,   int16_t)
#endif
#ifdef SIRIN_USE_ARRAY_I32
SIRIN_ARRAY_DECL(I32,   i32,   int32_t)
#endif
#ifdef SIRIN_USE_ARRAY_I64
SIRIN_ARRAY_DECL(I64,   i64,   int64_t)
#endif
#ifdef SIRIN_USE_ARRAY_FLOAT
SIRIN_ARRAY_DECL(Float, float, double)
#endif
#ifdef SIRIN_USE_ARRAY_BOOL
SIRIN_ARRAY_DECL(Bool,  bool,  int)
#endif

#endif
