#ifndef SIRIN_RUNTIME_H
#define SIRIN_RUNTIME_H

#include <stdint.h>
#include <stddef.h>
#include <stdlib.h> /* malloc — used by Option `Some(..)` heap-boxing */

/* ── SirinStr (always present) ────────────────────────────────────────────── */
typedef struct {
    char*  ptr;
    size_t len;
} SirinStr;

SirinStr sirin_str_new(const char* literal);
SirinStr sirin_str_copy(SirinStr s);
void     sirin_str_free(SirinStr s);
int      sirin_str_eq(SirinStr a, SirinStr b);

/* ── str ops on plain const char* (the language `str` type) ───────────────── */
const char* sirin_str_clone(const char* s);   /* deep copy — backs `:=` clone   */
void        sirin_cstr_free(const char* s);   /* free an owned heap str (Drop)  */
int64_t     sirin_str_len(const char* s);
const char* sirin_str_char_at(const char* s, int64_t i);
const char* sirin_str_slice(const char* s, int64_t start, int64_t end);
int64_t     sirin_str_index_of(const char* s, const char* sub);
int         sirin_str_contains(const char* s, const char* sub);
int         sirin_str_starts_with(const char* s, const char* pre);
int         sirin_str_ends_with(const char* s, const char* suf);
const char* sirin_str_trim(const char* s);
int64_t     sirin_str_to_int(const char* s);
double      sirin_str_to_float(const char* s);
const char* sirin_str_to_upper(const char* s);
const char* sirin_str_to_lower(const char* s);
const char* sirin_str_replace(const char* s, const char* from, const char* to);

/* ── minimal JSON field extraction (drives typed `.to_object()`) ──────────── */
const char* sirin_json_get_str(const char* json, const char* key);
int64_t     sirin_json_get_int(const char* json, const char* key);
double      sirin_json_get_float(const char* json, const char* key);
int         sirin_json_get_bool(const char* json, const char* key);

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
SirinVecStr sirin_str_split(const char* s, const char* sep);
#endif

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

/* ── Map[str, T] ──────────────────────────────────────────────────────────── */
#ifdef SIRIN_USE_MAP_STR_INT
typedef struct { char** keys; int64_t*     vals; size_t len; size_t cap; } SirinMapStrInt;
SirinMapStrInt  sirin_map_str_int_new(void);
void            sirin_map_str_int_insert(SirinMapStrInt* m, const char* key, int64_t value);
int64_t         sirin_map_str_int_get(SirinMapStrInt* m, const char* key);
void            sirin_map_str_int_free(SirinMapStrInt* m);
#endif

#ifdef SIRIN_USE_MAP_STR_STR
typedef struct { char** keys; SirinCStr*   vals; size_t len; size_t cap; } SirinMapStrStr;
SirinMapStrStr  sirin_map_str_str_new(void);
void            sirin_map_str_str_insert(SirinMapStrStr* m, const char* key, SirinCStr value);
SirinCStr       sirin_map_str_str_get(SirinMapStrStr* m, const char* key);
void            sirin_map_str_str_free(SirinMapStrStr* m);
#endif

#ifdef SIRIN_USE_MAP_STR_FLOAT
typedef struct { char** keys; double*      vals; size_t len; size_t cap; } SirinMapStrFloat;
SirinMapStrFloat sirin_map_str_float_new(void);
void             sirin_map_str_float_insert(SirinMapStrFloat* m, const char* key, double value);
double           sirin_map_str_float_get(SirinMapStrFloat* m, const char* key);
void             sirin_map_str_float_free(SirinMapStrFloat* m);
#endif

/* ── sirin.io ──────────────────────────────────────────────────────────────── */
#include <stdio.h>
#include <string.h>
const char* sirin_readln(void);

#endif
