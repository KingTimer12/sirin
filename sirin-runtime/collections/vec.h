/* Vec[T]: growable array. Only the element types a program uses are
   compiled in, selected by SIRIN_USE_VEC_<T> in sirin_config.h. */
#ifndef SIRIN_COLLECTIONS_VEC_H
#define SIRIN_COLLECTIONS_VEC_H

#include "../sirin_config.h"
#include "../core/str.h"

#include <stddef.h>
#include <stdint.h>

#define SIRIN_VEC_DECL(TypeName, FuncName, CType)                              \
typedef struct {                                                                \
    CType*  ptr;                                                                \
    size_t  len;                                                                \
    size_t  cap;                                                                \
} SirinVec##TypeName;                                                           \
SirinVec##TypeName sirin_vec_##FuncName##_new(size_t initial_cap);              \
void  sirin_vec_##FuncName##_push(SirinVec##TypeName* v, CType value);          \
CType sirin_vec_##FuncName##_get(SirinVec##TypeName* v, size_t index);          \
void  sirin_vec_##FuncName##_free(SirinVec##TypeName* v);

#ifdef SIRIN_USE_VEC_INT
SIRIN_VEC_DECL(Int,   int,   int64_t)
#endif
#ifdef SIRIN_USE_VEC_U8
SIRIN_VEC_DECL(U8,    u8,    uint8_t)
#endif
#ifdef SIRIN_USE_VEC_U16
SIRIN_VEC_DECL(U16,   u16,   uint16_t)
#endif
#ifdef SIRIN_USE_VEC_U32
SIRIN_VEC_DECL(U32,   u32,   uint32_t)
#endif
#ifdef SIRIN_USE_VEC_U64
SIRIN_VEC_DECL(U64,   u64,   uint64_t)
#endif
#ifdef SIRIN_USE_VEC_I8
SIRIN_VEC_DECL(I8,    i8,    int8_t)
#endif
#ifdef SIRIN_USE_VEC_I16
SIRIN_VEC_DECL(I16,   i16,   int16_t)
#endif
#ifdef SIRIN_USE_VEC_I32
SIRIN_VEC_DECL(I32,   i32,   int32_t)
#endif
#ifdef SIRIN_USE_VEC_I64
SIRIN_VEC_DECL(I64,   i64,   int64_t)
#endif
#ifdef SIRIN_USE_VEC_FLOAT
SIRIN_VEC_DECL(Float, float, double)
#endif
#ifdef SIRIN_USE_VEC_BOOL
SIRIN_VEC_DECL(Bool,  bool,  int)
#endif
#ifdef SIRIN_USE_VEC_STR
SIRIN_VEC_DECL(Str,   str,   SirinCStr)
/* `str.split(sep)`: lives here because it builds a Vec[str]. */
SirinVecStr sirin_str_split(const char* s, const char* sep);
#endif

#endif
