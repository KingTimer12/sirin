/* Minimal JSON field extraction from a flat object, used by typed
   `.to_object()`. Missing or mistyped fields read as "" / 0 / false. */
#ifndef SIRIN_CORE_JSON_H
#define SIRIN_CORE_JSON_H

#include <stdint.h>

const char* sirin_json_get_str(const char* json, const char* key);
int64_t     sirin_json_get_int(const char* json, const char* key);
double      sirin_json_get_float(const char* json, const char* key);
int         sirin_json_get_bool(const char* json, const char* key);

#endif
