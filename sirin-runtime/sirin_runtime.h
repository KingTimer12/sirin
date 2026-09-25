/* Sirin runtime: the C support library compiled into every Sirin program.
 *
 * Generated programs include this header, plus sirin_async.h and
 * sirin_net.h when they `use sirin.async` / `use sirin.net`. Each area
 * lives in its own directory:
 *
 *   core/         mem (allocation), str, json, io
 *   collections/  vec, array, set, map
 *   async/        sched (coroutines), channel, context_* (per platform)
 *   net/          tcp, udp, sys (per-platform socket shim)
 *
 * sirin_config.h is written by the compiler for each build and selects the
 * optional parts (collection element types, async support).
 */
#ifndef SIRIN_RUNTIME_H
#define SIRIN_RUNTIME_H

#include "sirin_config.h"

#include <stdint.h>
#include <stddef.h>
#include <stdlib.h> /* malloc: Option `Some(..)` boxes its value */

#include "core/str.h"
#include "core/json.h"
#include "core/io.h"
#include "collections/vec.h"
#include "collections/array.h"
#include "collections/set.h"
#include "collections/map.h"

#endif
