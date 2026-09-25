#include "map.h"
#include "../core/mem.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

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
    m->keys[m->len] = sirin_dup(key, strlen(key));                                \
    m->vals[m->len] = value;                                                       \
    m->len++;                                                                      \
}                                                                                  \
ValCType* sirin_map_str_##FuncName##_get_opt(SirinMapStr##TypeName* m,            \
                                             const char* key) {                    \
    for (size_t i = 0; i < m->len; i++) {                                         \
        if (strcmp(m->keys[i], key) == 0) return &m->vals[i];                     \
    }                                                                              \
    return NULL;                                                                   \
}                                                                                  \
ValCType sirin_map_str_##FuncName##_get(SirinMapStr##TypeName* m, const char* key) { \
    ValCType* v = sirin_map_str_##FuncName##_get_opt(m, key);                     \
    if (v) return *v;                                                              \
    fprintf(stderr, "sirin runtime: map key not found: \"%s\"\n", key);           \
    exit(1);                                                                       \
}                                                                                  \
int64_t sirin_map_str_##FuncName##_len(SirinMapStr##TypeName* m) {                \
    return (int64_t)m->len;                                                        \
}                                                                                  \
const char** sirin_map_str_##FuncName##_key_at(SirinMapStr##TypeName* m,          \
                                               int64_t i) {                        \
    if (i < 0 || (size_t)i >= m->len) return NULL;                                \
    return (const char**)&m->keys[i];                                             \
}                                                                                  \
void sirin_map_str_##FuncName##_free(SirinMapStr##TypeName* m) {                  \
    for (size_t i = 0; i < m->len; i++) free(m->keys[i]);                         \
    free(m->keys); free(m->vals);                                                  \
    m->keys = NULL; m->vals = NULL; m->len = 0; m->cap = 0;                       \
}

#ifdef SIRIN_USE_MAP_STR_INT
SIRIN_MAP_STR_IMPL(Int,   int,   int64_t)
#endif
#ifdef SIRIN_USE_MAP_STR_STR
SIRIN_MAP_STR_IMPL(Str,   str,   SirinCStr)
#endif
#ifdef SIRIN_USE_MAP_STR_FLOAT
SIRIN_MAP_STR_IMPL(Float, float, double)
#endif
