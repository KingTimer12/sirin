/* Map[str, T]: string keys, linear lookup. Keys are copied on insert.
   Enabled by SIRIN_USE_MAP_STR_<T> in sirin_config.h. */
#ifndef SIRIN_COLLECTIONS_MAP_H
#define SIRIN_COLLECTIONS_MAP_H

#include "../sirin_config.h"
#include "../core/str.h"

#include <stddef.h>
#include <stdint.h>

#define SIRIN_MAP_STR_DECL(TypeName, FuncName, ValCType)                                    \
typedef struct { char** keys; ValCType* vals; size_t len; size_t cap; } SirinMapStr##TypeName; \
SirinMapStr##TypeName sirin_map_str_##FuncName##_new(void);                                   \
void         sirin_map_str_##FuncName##_insert(SirinMapStr##TypeName* m, const char* key,      \
                                               ValCType value);                               \
ValCType     sirin_map_str_##FuncName##_get(SirinMapStr##TypeName* m, const char* key);        \
ValCType*    sirin_map_str_##FuncName##_get_opt(SirinMapStr##TypeName* m, const char* key);    \
int64_t      sirin_map_str_##FuncName##_len(SirinMapStr##TypeName* m);                         \
const char** sirin_map_str_##FuncName##_key_at(SirinMapStr##TypeName* m, int64_t i);           \
void         sirin_map_str_##FuncName##_free(SirinMapStr##TypeName* m);

#ifdef SIRIN_USE_MAP_STR_INT
SIRIN_MAP_STR_DECL(Int,   int,   int64_t)
#endif
#ifdef SIRIN_USE_MAP_STR_STR
SIRIN_MAP_STR_DECL(Str,   str,   SirinCStr)
#endif
#ifdef SIRIN_USE_MAP_STR_FLOAT
SIRIN_MAP_STR_DECL(Float, float, double)
#endif

#endif
