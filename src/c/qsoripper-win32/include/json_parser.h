/* json_parser.h — Minimal JSON value extractor for QsoRipper Win32
 *
 * KNOWN LIMITATIONS (by design for speed):
 *   - Key lookup uses strstr: matches the FIRST textual occurrence of
 *     "key" anywhere in the JSON. NOT depth-aware — nested objects with
 *     duplicate key names will return the wrong value.
 *   - String values are returned raw; JSON escape sequences (\n, \uXXXX,
 *     \\, \") are NOT decoded.
 *   - json_array_nth and json_array_count only work on arrays of objects
 *     ([ {...}, {...} ]). Primitive arrays are not supported.
 *   - Not a full JSON parser. Designed for flat, predictable JSON from
 *     QsoRipper CLI output where these limitations do not matter.
 */
#ifndef JSON_PARSER_H
#define JSON_PARSER_H

#include <stddef.h>

/* Finds "key": "value" or "key": number/bool in a JSON string.
   Returns a malloc'd string with the value, or NULL. Caller must free(). */
char *json_get_string(const char *json, const char *key);

/* Finds "key": <number> and returns the double value, or dflt if not found. */
double json_get_double(const char *json, const char *key, double dflt);

/* Finds "key": <int> and returns the int value, or dflt if not found. */
int json_get_int(const char *json, const char *key, int dflt);

/* Returns a pointer into json at the start of the nth object in the first
   array found. Returns NULL if not found. Does NOT allocate. */
const char *json_array_nth(const char *json, int n);

/* Extracts the object starting at 'start' (must point at '{').
   Returns a malloc'd string with the object, or NULL. Caller must free(). */
char *json_extract_object(const char *start);

#endif /* JSON_PARSER_H */
