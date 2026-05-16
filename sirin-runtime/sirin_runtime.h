#ifndef SIRIN_RUNTIME_H
#define SIRIN_RUNTIME_H

#include <stdint.h>
#include <stddef.h>

/* ── SirinStr ─────────────────────────────────────────────────────────────── */
typedef struct {
    char*  ptr;
    size_t len;
} SirinStr;

SirinStr sirin_str_new(const char* literal);
SirinStr sirin_str_copy(SirinStr s);
void     sirin_str_free(SirinStr s);
int      sirin_str_eq(SirinStr a, SirinStr b);

/* Pointer alias so const char* works as a macro CType without breaking CType* */
typedef const char* SirinCStr;

/* ── Vec ──────────────────────────────────────────────────────────────────── */
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

SIRIN_VEC_DECL(Int,   int,   int64_t)
SIRIN_VEC_DECL(U8,    u8,    uint8_t)
SIRIN_VEC_DECL(U16,   u16,   uint16_t)
SIRIN_VEC_DECL(U32,   u32,   uint32_t)
SIRIN_VEC_DECL(U64,   u64,   uint64_t)
SIRIN_VEC_DECL(I8,    i8,    int8_t)
SIRIN_VEC_DECL(I16,   i16,   int16_t)
SIRIN_VEC_DECL(I32,   i32,   int32_t)
SIRIN_VEC_DECL(I64,   i64,   int64_t)
SIRIN_VEC_DECL(Float, float, double)
SIRIN_VEC_DECL(Bool,  bool,  int)
SIRIN_VEC_DECL(Str,   str,   SirinCStr)

/* ── Array (fixed-size literal; same layout as Vec) ───────────────────────── */
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

SIRIN_ARRAY_DECL(Int,   int,   int64_t)
SIRIN_ARRAY_DECL(U8,    u8,    uint8_t)
SIRIN_ARRAY_DECL(U16,   u16,   uint16_t)
SIRIN_ARRAY_DECL(U32,   u32,   uint32_t)
SIRIN_ARRAY_DECL(U64,   u64,   uint64_t)
SIRIN_ARRAY_DECL(I8,    i8,    int8_t)
SIRIN_ARRAY_DECL(I16,   i16,   int16_t)
SIRIN_ARRAY_DECL(I32,   i32,   int32_t)
SIRIN_ARRAY_DECL(I64,   i64,   int64_t)
SIRIN_ARRAY_DECL(Float, float, double)
SIRIN_ARRAY_DECL(Bool,  bool,  int)
SIRIN_ARRAY_DECL(Str,   str,   SirinCStr)

/* ── Set (== comparison; numeric and bool) ────────────────────────────────── */
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

SIRIN_SET_DECL(Int,   int,   int64_t)
SIRIN_SET_DECL(U8,    u8,    uint8_t)
SIRIN_SET_DECL(U16,   u16,   uint16_t)
SIRIN_SET_DECL(U32,   u32,   uint32_t)
SIRIN_SET_DECL(U64,   u64,   uint64_t)
SIRIN_SET_DECL(I8,    i8,    int8_t)
SIRIN_SET_DECL(I16,   i16,   int16_t)
SIRIN_SET_DECL(I32,   i32,   int32_t)
SIRIN_SET_DECL(I64,   i64,   int64_t)
SIRIN_SET_DECL(Float, float, double)
SIRIN_SET_DECL(Bool,  bool,  int)

/* ── Map[str, T] ──────────────────────────────────────────────────────────── */
typedef struct { char** keys; int64_t*     vals; size_t len; size_t cap; } SirinMapStrInt;
SirinMapStrInt  sirin_map_str_int_new(void);
void            sirin_map_str_int_insert(SirinMapStrInt* m, const char* key, int64_t value);
int64_t         sirin_map_str_int_get(SirinMapStrInt* m, const char* key);
void            sirin_map_str_int_free(SirinMapStrInt* m);

typedef struct { char** keys; SirinCStr*   vals; size_t len; size_t cap; } SirinMapStrStr;
SirinMapStrStr  sirin_map_str_str_new(void);
void            sirin_map_str_str_insert(SirinMapStrStr* m, const char* key, SirinCStr value);
SirinCStr       sirin_map_str_str_get(SirinMapStrStr* m, const char* key);
void            sirin_map_str_str_free(SirinMapStrStr* m);

typedef struct { char** keys; double*      vals; size_t len; size_t cap; } SirinMapStrFloat;
SirinMapStrFloat sirin_map_str_float_new(void);
void             sirin_map_str_float_insert(SirinMapStrFloat* m, const char* key, double value);
double           sirin_map_str_float_get(SirinMapStrFloat* m, const char* key);
void             sirin_map_str_float_free(SirinMapStrFloat* m);

#endif
