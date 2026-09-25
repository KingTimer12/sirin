/* Strings. The language `str` is a NUL-terminated `const char*`; every
   operation that builds a new string returns a heap copy the caller owns. */
#ifndef SIRIN_CORE_STR_H
#define SIRIN_CORE_STR_H

#include <stddef.h>
#include <stdint.h>

/* Pointer alias so `const char*` works as a macro element type (CType*). */
typedef const char* SirinCStr;

/* Length-prefixed string. */
typedef struct {
    char*  ptr;
    size_t len;
} SirinStr;

SirinStr sirin_str_new(const char* literal);
SirinStr sirin_str_copy(SirinStr s);
void     sirin_str_free(SirinStr s);
int      sirin_str_eq(SirinStr a, SirinStr b);

const char* sirin_str_clone(const char* s);                  /* backs `:=` and `::clone` */
void        sirin_cstr_free(const char* s);                  /* drops an owned str       */
const char* sirin_str_concat(const char* a, const char* b);  /* backs `str + str`        */
const char* sirin_int_to_str(int64_t n);                     /* backs `int.to_str()`     */

int64_t     sirin_str_len(const char* s);
int64_t     sirin_str_index_of(const char* s, const char* sub);
int         sirin_str_contains(const char* s, const char* sub);
int         sirin_str_starts_with(const char* s, const char* pre);
int         sirin_str_ends_with(const char* s, const char* suf);

const char* sirin_str_char_at(const char* s, int64_t i);
const char* sirin_str_slice(const char* s, int64_t start, int64_t end);
const char* sirin_str_trim(const char* s);
const char* sirin_str_to_upper(const char* s);
const char* sirin_str_to_lower(const char* s);
const char* sirin_str_replace(const char* s, const char* from, const char* to);

int64_t     sirin_str_to_int(const char* s);
double      sirin_str_to_float(const char* s);

#endif
