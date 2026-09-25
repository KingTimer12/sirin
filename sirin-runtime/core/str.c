#include "str.h"
#include "mem.h"

#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* ── SirinStr ─────────────────────────────────────────────────────────────── */

SirinStr sirin_str_new(const char* literal) {
    size_t len = strlen(literal);
    char* buf = (char*)sirin_alloc(len);
    memcpy(buf, literal, len);
    return (SirinStr){ .ptr = buf, .len = len };
}

SirinStr sirin_str_copy(SirinStr s) {
    char* buf = (char*)sirin_alloc(s.len);
    memcpy(buf, s.ptr, s.len);
    return (SirinStr){ .ptr = buf, .len = s.len };
}

void sirin_str_free(SirinStr s) { free(s.ptr); }

int sirin_str_eq(SirinStr a, SirinStr b) {
    return a.len == b.len && memcmp(a.ptr, b.ptr, a.len) == 0;
}

/* ── ownership ────────────────────────────────────────────────────────────── */

const char* sirin_str_clone(const char* s) { return sirin_dup(s, strlen(s)); }

void sirin_cstr_free(const char* s) { free((void*)s); }

const char* sirin_str_concat(const char* a, const char* b) {
    size_t la = strlen(a), lb = strlen(b);
    char* out = (char*)sirin_alloc(la + lb + 1);
    memcpy(out, a, la);
    memcpy(out + la, b, lb);
    out[la + lb] = '\0';
    return out;
}

const char* sirin_int_to_str(int64_t n) {
    char buf[32];
    int len = snprintf(buf, sizeof(buf), "%lld", (long long)n);
    return sirin_dup(buf, (size_t)len);
}

/* ── queries ──────────────────────────────────────────────────────────────── */

int64_t sirin_str_len(const char* s) { return (int64_t)strlen(s); }

int64_t sirin_str_index_of(const char* s, const char* sub) {
    const char* p = strstr(s, sub);
    return p ? (int64_t)(p - s) : -1;
}

int sirin_str_contains(const char* s, const char* sub) {
    return strstr(s, sub) != NULL;
}

int sirin_str_starts_with(const char* s, const char* pre) {
    return strncmp(s, pre, strlen(pre)) == 0;
}

int sirin_str_ends_with(const char* s, const char* suf) {
    size_t ls = strlen(s), lf = strlen(suf);
    if (lf > ls) return 0;
    return memcmp(s + ls - lf, suf, lf) == 0;
}

/* ── new strings ──────────────────────────────────────────────────────────── */

const char* sirin_str_char_at(const char* s, int64_t i) {
    int64_t n = (int64_t)strlen(s);
    if (i < 0 || i >= n) return sirin_dup("", 0);
    return sirin_dup(s + i, 1);
}

const char* sirin_str_slice(const char* s, int64_t start, int64_t end) {
    int64_t n = (int64_t)strlen(s);
    if (start < 0) start += n;          /* negative = from end */
    if (end   < 0) end   += n;
    if (start < 0) start = 0;
    if (end   > n) end   = n;
    if (start >= end) return sirin_dup("", 0);
    return sirin_dup(s + start, (size_t)(end - start));
}

const char* sirin_str_trim(const char* s) {
    while (*s && isspace((unsigned char)*s)) s++;
    const char* end = s + strlen(s);
    while (end > s && isspace((unsigned char)end[-1])) end--;
    return sirin_dup(s, (size_t)(end - s));
}

static const char* map_chars(const char* s, int (*f)(int)) {
    size_t n = strlen(s);
    char* buf = (char*)sirin_alloc(n + 1);
    for (size_t i = 0; i < n; i++) buf[i] = (char)f((unsigned char)s[i]);
    buf[n] = '\0';
    return buf;
}

const char* sirin_str_to_upper(const char* s) { return map_chars(s, toupper); }
const char* sirin_str_to_lower(const char* s) { return map_chars(s, tolower); }

const char* sirin_str_replace(const char* s, const char* from, const char* to) {
    size_t lf = strlen(from);
    if (lf == 0) return sirin_dup(s, strlen(s));
    size_t lt = strlen(to), ls = strlen(s), count = 0;
    for (const char* p = s; (p = strstr(p, from)); p += lf) count++;
    char* buf = (char*)sirin_alloc(ls + count * (lt > lf ? lt - lf : 0) + 1);
    char* out = buf;
    const char* p = s;
    for (;;) {
        const char* hit = strstr(p, from);
        if (!hit) { memcpy(out, p, strlen(p) + 1); break; }
        size_t chunk = (size_t)(hit - p);
        memcpy(out, p, chunk); out += chunk;
        memcpy(out, to, lt);   out += lt;
        p = hit + lf;
    }
    return buf;
}

/* ── conversions ──────────────────────────────────────────────────────────── */

int64_t sirin_str_to_int(const char* s)   { return (int64_t)strtoll(s, NULL, 10); }
double  sirin_str_to_float(const char* s) { return strtod(s, NULL); }
