#include "json.h"
#include "mem.h"

#include <ctype.h>
#include <stdlib.h>
#include <string.h>

/* Position of the value for "key": the first non-space char after its colon,
   or NULL if the key is absent. */
static const char* find_value(const char* json, const char* key) {
    size_t lk = strlen(key);
    const char* p = json;
    while ((p = strchr(p, '"'))) {
        const char* kstart = p + 1;
        const char* kend = strchr(kstart, '"');
        if (!kend) return NULL;
        size_t klen = (size_t)(kend - kstart);
        const char* after = kend + 1;
        while (*after && isspace((unsigned char)*after)) after++;
        if (*after == ':' && klen == lk && memcmp(kstart, key, lk) == 0) {
            after++;
            while (*after && isspace((unsigned char)*after)) after++;
            return after;
        }
        p = kend + 1;
    }
    return NULL;
}

const char* sirin_json_get_str(const char* json, const char* key) {
    const char* v = find_value(json, key);
    if (!v || *v != '"') return sirin_dup("", 0);
    const char* start = v + 1;
    const char* p = start;
    while (*p && *p != '"') { if (*p == '\\' && p[1]) p++; p++; }
    return sirin_dup(start, (size_t)(p - start));
}

int64_t sirin_json_get_int(const char* json, const char* key) {
    const char* v = find_value(json, key);
    return v ? (int64_t)strtoll(v, NULL, 10) : 0;
}

double sirin_json_get_float(const char* json, const char* key) {
    const char* v = find_value(json, key);
    return v ? strtod(v, NULL) : 0.0;
}

int sirin_json_get_bool(const char* json, const char* key) {
    const char* v = find_value(json, key);
    return v && strncmp(v, "true", 4) == 0;
}
