/* json_parser.c — Minimal JSON value extractor for QsoRipper Win32
 *
 * Extracted from main.c and hardened:
 *   - NULL checks on all public entry points
 *   - Full JSON whitespace handling (space, tab, CR, LF) after ':'
 *   - Safe numeric parsing via strtol/strtod (no undefined behavior on overflow)
 *   - String-aware brace/bracket matching (braces inside quoted strings are
 *     correctly ignored by extract_object, array_nth)
 *   - Dynamic pattern buffer for keys longer than 126 characters
 *   - Zero-allocation numeric parsing (get_int/get_double parse in-place)
 */

#define _CRT_SECURE_NO_WARNINGS
#include "json_parser.h"
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <errno.h>
#include <limits.h>

/* Returns non-zero if c is JSON whitespace */
static int is_json_ws(char c)
{
    return c == ' ' || c == '\t' || c == '\r' || c == '\n';
}

char *json_get_string(const char *json, const char *key)
{
    if (!json || !key) return NULL;

    /* Build the quoted key pattern: "key" */
    size_t key_len = strlen(key);
    size_t pat_len = key_len + 2; /* two quotes */
    char stack_buf[128];
    char *pattern;
    if (pat_len < sizeof(stack_buf)) {
        pattern = stack_buf;
    } else {
        pattern = (char *)malloc(pat_len + 1);
        if (!pattern) return NULL;
    }
    pattern[0] = '"';
    memcpy(pattern + 1, key, key_len);
    pattern[1 + key_len] = '"';
    pattern[2 + key_len] = '\0';

    const char *p = strstr(json, pattern);
    if (pattern != stack_buf) free(pattern);
    if (!p) return NULL;

    p += pat_len;
    /* Skip JSON whitespace and colon */
    while (is_json_ws(*p)) p++;
    if (*p == ':') p++;
    while (is_json_ws(*p)) p++;

    if (*p == '"') {
        p++;
        const char *end = p;
        while (*end && *end != '"') {
            if (*end == '\\' && *(end + 1)) end++;
            end++;
        }
        size_t len = (size_t)(end - p);
        char *val = (char *)malloc(len + 1);
        if (!val) return NULL;
        memcpy(val, p, len);
        val[len] = 0;
        return val;
    }
    /* Numeric or boolean value */
    const char *end = p;
    while (*end && *end != ',' && *end != '}' && *end != ']' && *end != '\n') end++;
    size_t len = (size_t)(end - p);
    while (len > 0 && (p[len - 1] == ' ' || p[len - 1] == '\r')) len--;
    char *val = (char *)malloc(len + 1);
    if (!val) return NULL;
    memcpy(val, p, len);
    val[len] = 0;
    return val;
}

/* Locate the value span for a key without allocating.
   Returns pointer to start of value, sets *out_len to length.
   Returns NULL if not found. */
static const char *locate_value(const char *json, const char *key, size_t *out_len)
{
    if (!json || !key) return NULL;

    size_t key_len = strlen(key);
    size_t pat_len = key_len + 2;
    char stack_buf[128];
    char *pattern;
    if (pat_len < sizeof(stack_buf)) {
        pattern = stack_buf;
    } else {
        pattern = (char *)malloc(pat_len + 1);
        if (!pattern) return NULL;
    }
    pattern[0] = '"';
    memcpy(pattern + 1, key, key_len);
    pattern[1 + key_len] = '"';
    pattern[2 + key_len] = '\0';

    const char *p = strstr(json, pattern);
    if (pattern != stack_buf) free(pattern);
    if (!p) return NULL;

    p += pat_len;
    while (is_json_ws(*p)) p++;
    if (*p == ':') p++;
    while (is_json_ws(*p)) p++;

    /* Find value end */
    if (*p == '"') {
        p++;
        const char *end = p;
        while (*end && *end != '"') {
            if (*end == '\\' && *(end + 1)) end++;
            end++;
        }
        *out_len = (size_t)(end - p);
        return p;
    }
    /* Numeric/boolean value */
    const char *end = p;
    while (*end && *end != ',' && *end != '}' && *end != ']' && *end != '\n') end++;
    size_t len = (size_t)(end - p);
    while (len > 0 && (p[len - 1] == ' ' || p[len - 1] == '\r')) len--;
    *out_len = len;
    return p;
}

double json_get_double(const char *json, const char *key, double dflt)
{
    size_t len;
    const char *span = locate_value(json, key, &len);
    if (!span || len == 0) return dflt;

    /* Copy to a small stack buffer for strtod (needs NUL terminator) */
    char buf[64];
    if (len >= sizeof(buf)) return dflt;
    memcpy(buf, span, len);
    buf[len] = '\0';

    char *endp;
    errno = 0;
    double r = strtod(buf, &endp);
    if (endp == buf || errno == ERANGE) return dflt;
    return r;
}

int json_get_int(const char *json, const char *key, int dflt)
{
    size_t len;
    const char *span = locate_value(json, key, &len);
    if (!span || len == 0) return dflt;

    char buf[32];
    if (len >= sizeof(buf)) return dflt;
    memcpy(buf, span, len);
    buf[len] = '\0';

    char *endp;
    errno = 0;
    long r = strtol(buf, &endp, 10);
    if (endp == buf || errno == ERANGE || r > INT_MAX || r < INT_MIN) return dflt;
    return (int)r;
}

const char *json_array_nth(const char *json, int n)
{
    if (!json) return NULL;
    const char *p = strchr(json, '[');
    if (!p) return NULL;
    p++;
    int depth = 0, idx = 0, in_str = 0;
    for (; *p; p++) {
        if (in_str) {
            if (in_str == 2) { in_str = 1; continue; } /* escaped char */
            if (*p == '\\') { in_str = 2; continue; }
            if (*p == '"') { in_str = 0; }
            continue;
        }
        if (*p == '"') { in_str = 1; continue; }
        if (*p == '{') {
            if (depth == 0 && idx == n) return p;
            depth++;
        } else if (*p == '}') {
            depth--;
        } else if (*p == ',' && depth == 0) {
            idx++;
        } else if (*p == ']' && depth == 0) {
            break;
        }
    }
    return NULL;
}

char *json_extract_object(const char *start)
{
    if (!start || *start != '{') return NULL;
    int depth = 0, in_str = 0;
    const char *p = start;
    for (; *p; p++) {
        if (in_str) {
            if (in_str == 2) { in_str = 1; continue; }
            if (*p == '\\') { in_str = 2; continue; }
            if (*p == '"') { in_str = 0; }
            continue;
        }
        if (*p == '"') { in_str = 1; continue; }
        if (*p == '{') depth++;
        else if (*p == '}') { depth--; if (depth == 0) break; }
    }
    if (depth != 0) return NULL;
    size_t len = (size_t)(p - start + 1);
    char *obj = (char *)malloc(len + 1);
    if (!obj) return NULL;
    memcpy(obj, start, len);
    obj[len] = 0;
    return obj;
}
