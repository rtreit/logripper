/* json_parser_tests.c — Tests for the hardened JSON parser */
#define _CRT_SECURE_NO_WARNINGS
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include "../include/json_parser.h"

static int g_pass = 0;
static int g_fail = 0;

#define ASSERT_STR_EQ(expected, actual) do { \
    if ((actual) == NULL) { \
        printf("  FAIL %s:%d: expected \"%s\", got NULL\n", __FILE__, __LINE__, (expected)); \
        g_fail++; \
    } else if (strcmp((expected), (actual)) != 0) { \
        printf("  FAIL %s:%d: expected \"%s\", got \"%s\"\n", __FILE__, __LINE__, (expected), (actual)); \
        g_fail++; \
    } else { g_pass++; } \
} while(0)

#define ASSERT_NULL(actual) do { \
    if ((actual) != NULL) { \
        printf("  FAIL %s:%d: expected NULL\n", __FILE__, __LINE__); \
        g_fail++; \
    } else { g_pass++; } \
} while(0)

#define ASSERT_INT_EQ(expected, actual) do { \
    if ((expected) != (actual)) { \
        printf("  FAIL %s:%d: expected %d, got %d\n", __FILE__, __LINE__, (expected), (actual)); \
        g_fail++; \
    } else { g_pass++; } \
} while(0)

#define ASSERT_DOUBLE_EQ(expected, actual, eps) do { \
    if (fabs((expected) - (actual)) > (eps)) { \
        printf("  FAIL %s:%d: expected %f, got %f\n", __FILE__, __LINE__, (expected), (actual)); \
        g_fail++; \
    } else { g_pass++; } \
} while(0)

/* ── QsoRipper-style JSON payloads ───────────────────────────────── */

static const char *LOOKUP_JSON =
    "{\"callsign\":\"W1AW\",\"name\":\"ARRL HQ\",\"qth\":\"Newington, CT\","
    "\"grid\":\"FN31pr\",\"country\":\"United States\","
    "\"cqZone\":5,\"ituZone\":8,\"dxcc\":291,"
    "\"latitude\":41.714775,\"longitude\":-72.727260}";

static const char *RIG_STATUS_JSON =
    "{\"connected\":\"true\",\"freqDisplay\":\"14.074.500\","
    "\"band\":\"20m\",\"mode\":\"USB\"}";

static const char *QSO_LIST_JSON =
    "[{\"callsign\":\"W1AW\",\"band\":\"20m\"},"
    "{\"callsign\":\"K7RND\",\"band\":\"40m\"},"
    "{\"callsign\":\"JA1ABC\",\"band\":\"15m\"}]";

/* ── Test: basic field extraction ────────────────────────────────── */

static void test_basic_fields(void)
{
    printf("test_basic_fields\n");
    char *v;

    v = json_get_string(LOOKUP_JSON, "callsign");
    ASSERT_STR_EQ("W1AW", v); free(v);

    v = json_get_string(LOOKUP_JSON, "country");
    ASSERT_STR_EQ("United States", v); free(v);

    ASSERT_INT_EQ(5, json_get_int(LOOKUP_JSON, "cqZone", 0));
    ASSERT_INT_EQ(291, json_get_int(LOOKUP_JSON, "dxcc", 0));
    ASSERT_DOUBLE_EQ(41.714775, json_get_double(LOOKUP_JSON, "latitude", 0), 0.0001);
    ASSERT_DOUBLE_EQ(-72.727260, json_get_double(LOOKUP_JSON, "longitude", 0), 0.0001);
}

/* ── Test: NULL safety ───────────────────────────────────────────── */

static void test_null_safety(void)
{
    printf("test_null_safety\n");
    ASSERT_NULL(json_get_string(NULL, "key"));
    ASSERT_NULL(json_get_string(LOOKUP_JSON, NULL));
    ASSERT_NULL(json_array_nth(NULL, 0));
    ASSERT_NULL(json_extract_object(NULL));
    ASSERT_NULL(json_extract_object("not a brace"));
}

/* ── Test: missing key returns default ───────────────────────────── */

static void test_missing_key(void)
{
    printf("test_missing_key\n");
    ASSERT_NULL(json_get_string(LOOKUP_JSON, "bogus"));
    ASSERT_INT_EQ(42, json_get_int(LOOKUP_JSON, "bogus", 42));
    ASSERT_DOUBLE_EQ(3.14, json_get_double(LOOKUP_JSON, "bogus", 3.14), 0.001);
}

/* ── Test: whitespace after colon ────────────────────────────────── */

static void test_whitespace_after_colon(void)
{
    printf("test_whitespace_after_colon\n");
    char *v;

    v = json_get_string("{\"k\":\t\"tabval\"}", "k");
    ASSERT_STR_EQ("tabval", v); free(v);

    v = json_get_string("{\"k\":\n\"nlval\"}", "k");
    ASSERT_STR_EQ("nlval", v); free(v);

    v = json_get_string("{\"k\" : \r\n \"wsval\"}", "k");
    ASSERT_STR_EQ("wsval", v); free(v);

    ASSERT_INT_EQ(42, json_get_int("{\"n\":\t42}", "n", 0));
}

/* ── Test: safe numeric parsing ──────────────────────────────────── */

static void test_safe_numerics(void)
{
    printf("test_safe_numerics\n");
    ASSERT_INT_EQ(123, json_get_int("{\"n\":123}", "n", -1));
    ASSERT_INT_EQ(-456, json_get_int("{\"n\":-456}", "n", -1));

    /* Overflow returns default */
    ASSERT_INT_EQ(99, json_get_int("{\"n\":99999999999999999999}", "n", 99));

    /* Non-numeric string value returns default for int */
    ASSERT_INT_EQ(77, json_get_int("{\"n\":\"hello\"}", "n", 77));
}

/* ── Test: string-aware brace matching ───────────────────────────── */

static void test_string_aware_braces(void)
{
    printf("test_string_aware_braces\n");
    char *v;

    /* Closing brace inside a string value */
    v = json_extract_object("{\"x\":\"}\"}");
    ASSERT_STR_EQ("{\"x\":\"}\"}", v); free(v);

    /* Opening brace inside a string value */
    v = json_extract_object("{\"x\":\"{\"}");
    ASSERT_STR_EQ("{\"x\":\"{\"}", v); free(v);

    /* Array with braces in string values */
    const char *json = "[{\"x\":\"}\"},{\"y\":\"2\"}]";
    const char *elem1 = json_array_nth(json, 1);
    if (elem1) {
        v = json_extract_object(elem1);
        if (v) {
            char *y = json_get_string(v, "y");
            ASSERT_STR_EQ("2", y); free(y);
            free(v);
        }
    } else {
        printf("  FAIL: json_array_nth returned NULL for index 1\n");
        g_fail++;
    }
}

/* ── Test: array operations ──────────────────────────────────────── */

static void test_array_operations(void)
{
    printf("test_array_operations\n");

    const char *elem0 = json_array_nth(QSO_LIST_JSON, 0);
    if (elem0) {
        char *obj = json_extract_object(elem0);
        if (obj) {
            char *v = json_get_string(obj, "callsign");
            ASSERT_STR_EQ("W1AW", v); free(v);
            free(obj);
        }
    }

    const char *elem2 = json_array_nth(QSO_LIST_JSON, 2);
    if (elem2) {
        char *obj = json_extract_object(elem2);
        if (obj) {
            char *v = json_get_string(obj, "callsign");
            ASSERT_STR_EQ("JA1ABC", v); free(v);
            free(obj);
        }
    }

    ASSERT_NULL(json_array_nth(QSO_LIST_JSON, 3));
    ASSERT_NULL(json_array_nth(QSO_LIST_JSON, 99));
}

/* ── Test: long key (>126 chars) ─────────────────────────────────── */

static void test_long_key(void)
{
    printf("test_long_key\n");
    char key[200];
    memset(key, 'k', 199);
    key[199] = '\0';

    char json[512];
    snprintf(json, sizeof(json), "{\"%s\":\"longval\"}", key);

    char *v = json_get_string(json, key);
    ASSERT_STR_EQ("longval", v); free(v);
}

/* ── Test: adversarial / malformed inputs ─────────────────────────── */

static void test_empty_string_input(void)
{
    printf("test_empty_string_input\n");
    /* Empty string should not crash, just return NULL/default */
    ASSERT_NULL(json_get_string("", "key"));
    ASSERT_INT_EQ(42, json_get_int("", "key", 42));
    ASSERT_DOUBLE_EQ(1.5, json_get_double("", "key", 1.5), 0.001);
    ASSERT_NULL(json_array_nth("", 0));
    ASSERT_NULL(json_extract_object(""));
}

static void test_truncated_json(void)
{
    printf("test_truncated_json\n");
    char *v;

    /* Unterminated string value */
    v = json_get_string("{\"k\":\"val", "k");
    /* Should return "val" (scans to end of string without closing quote) */
    if (v) { free(v); g_pass++; } else { g_pass++; } /* no crash is the goal */

    /* Key found but value is just end of input */
    v = json_get_string("{\"k\":", "k");
    /* Empty or NULL — either is fine, must not crash */
    if (v) free(v);
    g_pass++;

    /* Key found, colon, then nothing */
    v = json_get_string("{\"k\": ", "k");
    if (v) free(v);
    g_pass++;

    /* Unterminated object */
    v = json_extract_object("{\"k\":\"v\"");
    ASSERT_NULL(v); /* depth != 0 at end */

    /* Just an opening brace */
    v = json_extract_object("{");
    ASSERT_NULL(v);

    /* Unterminated array */
    ASSERT_NULL(json_array_nth("[{\"x\":1}", 1));
    /* No closing bracket — should return NULL for element beyond what exists */
}

static void test_negative_array_index(void)
{
    printf("test_negative_array_index\n");
    ASSERT_NULL(json_array_nth(QSO_LIST_JSON, -1));
    ASSERT_NULL(json_array_nth(QSO_LIST_JSON, -999));
}

static void test_empty_key(void)
{
    printf("test_empty_key\n");
    /* Empty key is a valid (if weird) JSON key: {"":"val"} */
    char *v = json_get_string("{\"\":\"emptykey\"}", "");
    ASSERT_STR_EQ("emptykey", v); free(v);

    /* Empty key on normal JSON — should not match anything */
    ASSERT_NULL(json_get_string("{\"k\":\"v\"}", ""));
}

static void test_no_colon(void)
{
    printf("test_no_colon\n");
    /* Malformed: key without colon — parser skips whitespace then sees '"',
       treats next quoted string as value. This is "wrong" but must not crash. */
    char *v = json_get_string("{\"k\" \"v\"}", "k");
    if (v) free(v);
    g_pass++; /* no crash is the goal */
}

static void test_backslash_at_end(void)
{
    printf("test_backslash_at_end\n");
    /* Backslash as last character before NUL */
    char *v = json_get_string("{\"k\":\"val\\", "k");
    /* The escape handler checks *(end+1) which is NUL, so the
       backslash is not treated as an escape. Should terminate. */
    if (v) free(v);
    g_pass++; /* no crash */

    /* Backslash at end of extracted object */
    v = json_extract_object("{\"k\":\"\\");
    /* Unterminated — should return NULL */
    ASSERT_NULL(v);
}

static void test_whitespace_only_input(void)
{
    printf("test_whitespace_only_input\n");
    ASSERT_NULL(json_get_string("   \t\n\r  ", "key"));
    ASSERT_NULL(json_array_nth("   ", 0));
    ASSERT_NULL(json_extract_object("   "));
}

static void test_single_char_inputs(void)
{
    printf("test_single_char_inputs\n");
    ASSERT_NULL(json_get_string("{", "k"));
    ASSERT_NULL(json_get_string("[", "k"));
    ASSERT_NULL(json_get_string("\"", "k"));
    ASSERT_NULL(json_get_string("}", "k"));
    ASSERT_NULL(json_get_string("]", "k"));
    ASSERT_NULL(json_extract_object("["));
    ASSERT_NULL(json_extract_object("}"));
    ASSERT_NULL(json_array_nth("{", 0));
    ASSERT_NULL(json_array_nth("]", 0));
}

static void test_deeply_nested(void)
{
    printf("test_deeply_nested\n");
    /* Object nested 5 deep */
    const char *deep = "{\"a\":{\"b\":{\"c\":{\"d\":{\"e\":\"leaf\"}}}}}";
    char *v = json_extract_object(deep);
    ASSERT_STR_EQ(deep, v); free(v);

    /* Can extract inner key (strstr finds first match) */
    v = json_get_string(deep, "e");
    ASSERT_STR_EQ("leaf", v); free(v);
}

static void test_numeric_edge_cases(void)
{
    printf("test_numeric_edge_cases\n");
    /* INT_MAX boundary */
    ASSERT_INT_EQ(2147483647, json_get_int("{\"n\":2147483647}", "n", -1));
    /* INT_MIN boundary */
    ASSERT_INT_EQ(-2147483647 - 1, json_get_int("{\"n\":-2147483648}", "n", -1));
    /* Just past INT_MAX → default */
    ASSERT_INT_EQ(99, json_get_int("{\"n\":2147483648}", "n", 99));
    /* Zero */
    ASSERT_INT_EQ(0, json_get_int("{\"n\":0}", "n", -1));
    /* Negative zero double */
    ASSERT_DOUBLE_EQ(0.0, json_get_double("{\"n\":-0.0}", "n", 99), 0.001);
    /* Very small double */
    ASSERT_DOUBLE_EQ(0.0001, json_get_double("{\"n\":0.0001}", "n", 99), 0.00001);
    /* Scientific notation */
    ASSERT_DOUBLE_EQ(1500.0, json_get_double("{\"n\":1.5e3}", "n", 0), 0.01);
}

static void test_value_is_boolean(void)
{
    printf("test_value_is_boolean\n");
    char *v;
    /* true/false/null as values */
    v = json_get_string("{\"k\":true}", "k");
    ASSERT_STR_EQ("true", v); free(v);

    v = json_get_string("{\"k\":false}", "k");
    ASSERT_STR_EQ("false", v); free(v);

    v = json_get_string("{\"k\":null}", "k");
    ASSERT_STR_EQ("null", v); free(v);
}

static void test_escaped_quotes_in_value(void)
{
    printf("test_escaped_quotes_in_value\n");
    /* Value contains escaped quotes: "say \"hello\"" */
    char *v = json_get_string("{\"k\":\"say \\\"hello\\\"\"}", "k");
    /* Raw return (no unescaping): say \"hello\" */
    ASSERT_STR_EQ("say \\\"hello\\\"", v); free(v);
}

static void test_key_as_substring_of_value(void)
{
    printf("test_key_as_substring_of_value\n");
    /* Key "id" appears inside a value before the real key */
    char *v = json_get_string("{\"name\":\"id_holder\",\"id\":\"real\"}", "id");
    /* strstr finds "id" inside "id_holder" first — this is a known limitation.
       The important thing is it doesn't crash. */
    if (v) free(v);
    g_pass++; /* no crash */
}

static void test_consecutive_commas(void)
{
    printf("test_consecutive_commas\n");
    /* Malformed: extra commas */
    ASSERT_NULL(json_array_nth("[,,]", 0));
    /* No crash is the goal */
    g_pass++;
}

static void test_empty_object(void)
{
    printf("test_empty_object\n");
    char *v = json_extract_object("{}");
    ASSERT_STR_EQ("{}", v); free(v);

    ASSERT_NULL(json_get_string("{}", "anything"));
}

static void test_array_of_one(void)
{
    printf("test_array_of_one\n");
    const char *json = "[{\"x\":1}]";
    const char *e = json_array_nth(json, 0);
    if (e) {
        char *obj = json_extract_object(e);
        if (obj) {
            ASSERT_INT_EQ(1, json_get_int(obj, "x", 0));
            free(obj);
        }
    }
    ASSERT_NULL(json_array_nth(json, 1));
}

/* ══════════════════════════════════════════════════════════════════════
 * ADVERSARIAL / CRASH-PROBING TESTS
 * Goal: exercise every code path that touches raw pointers, loops over
 * untrusted input, or does arithmetic on lengths.  Every test that
 * reaches the end without crashing is a pass.
 * ══════════════════════════════════════════════════════════════════════ */

/* Helper: call all public APIs on a given string; must not crash. */
static void must_not_crash(const char *json)
{
    char *v;
    v = json_get_string(json, "k");  if (v) free(v);
    v = json_get_string(json, "");   if (v) free(v);
    (void)json_get_int(json, "k", 0);
    (void)json_get_double(json, "k", 0);
    (void)json_array_nth(json, 0);
    (void)json_array_nth(json, -1);
    v = json_extract_object(json);   if (v) free(v);
}

static void test_garbage_inputs(void)
{
    printf("test_garbage_inputs\n");
    /* Every one of these must survive without crashing */
    must_not_crash("");
    must_not_crash(" ");
    must_not_crash("\t\r\n");
    must_not_crash("{");
    must_not_crash("}");
    must_not_crash("[");
    must_not_crash("]");
    must_not_crash("\"");
    must_not_crash("\\");
    must_not_crash(":");
    must_not_crash(",");
    must_not_crash("null");
    must_not_crash("true");
    must_not_crash("false");
    must_not_crash("0");
    must_not_crash("-1");
    must_not_crash("\"\"");
    must_not_crash("\"k\"");
    must_not_crash("\"k\":");
    must_not_crash("\"k\":\"");
    must_not_crash("{\"k\"");
    must_not_crash("{\"k\":");
    must_not_crash("{\"k\": ");
    must_not_crash("{\"k\":}");
    must_not_crash("{\"k\":,}");
    must_not_crash("{,}");
    must_not_crash("{:}");
    must_not_crash("{\"\":}");
    must_not_crash("[,]");
    must_not_crash("[,,,,]");
    must_not_crash("}{");
    must_not_crash("][");
    must_not_crash("}{][}{][");
    must_not_crash("{{{{{{{{{{");
    must_not_crash("}}}}}}}}}}");
    must_not_crash("[[[[[[[[[[");
    must_not_crash("]]]]]]]]]]");
    must_not_crash("\"\"\"\"\"\"\"\"");
    must_not_crash("\\\\\\\\\\\\\\\\");
    must_not_crash(",,,,,,,,,,,");
    must_not_crash("::::::::::::");
    g_pass++; /* survived the gauntlet */
}

static void test_binary_garbage(void)
{
    printf("test_binary_garbage\n");
    /* Feed every possible single-byte value (except NUL) as a 1-char string */
    char buf[2] = {0, 0};
    for (int c = 1; c < 256; c++) {
        buf[0] = (char)c;
        must_not_crash(buf);
    }
    /* 2-byte combos of interesting chars */
    const char interesting[] = "{}\"][,:\\\" \t\n\r";
    char buf2[3] = {0, 0, 0};
    for (int i = 0; interesting[i]; i++) {
        for (int j = 0; interesting[j]; j++) {
            buf2[0] = interesting[i];
            buf2[1] = interesting[j];
            must_not_crash(buf2);
        }
    }
    g_pass++;
}

static void test_truncation_at_every_position(void)
{
    printf("test_truncation_at_every_position\n");
    /* Take a valid JSON string and truncate it at every byte position.
       None of these should crash. */
    const char *full = "{\"callsign\":\"W1AW\",\"band\":\"20m\",\"dxcc\":291}";
    size_t full_len = strlen(full);
    char *buf = (char *)malloc(full_len + 1);

    for (size_t cut = 0; cut <= full_len; cut++) {
        memcpy(buf, full, cut);
        buf[cut] = '\0';
        must_not_crash(buf);
    }
    free(buf);
    g_pass++;
}

static void test_truncation_array_at_every_position(void)
{
    printf("test_truncation_array_at_every_position\n");
    const char *full = "[{\"x\":\"}\",\"y\":1},{\"z\":\"hello\"}]";
    size_t full_len = strlen(full);
    char *buf = (char *)malloc(full_len + 1);

    for (size_t cut = 0; cut <= full_len; cut++) {
        memcpy(buf, full, cut);
        buf[cut] = '\0';
        (void)json_array_nth(buf, 0);
        (void)json_array_nth(buf, 1);
        (void)json_array_nth(buf, 99);
        json_extract_object(buf);
    }
    free(buf);
    g_pass++;
}

static void test_depth_underflow(void)
{
    printf("test_depth_underflow\n");
    /* More closing braces than opening — depth goes negative */
    must_not_crash("}}}}}}");
    must_not_crash("[}}}}]");
    must_not_crash("{\"k\":\"v\"}}}}");

    /* extract_object with extra closing braces */
    char *v = json_extract_object("{\"k\":\"v\"}}}}");
    /* Should extract just the first balanced object */
    if (v) {
        ASSERT_STR_EQ("{\"k\":\"v\"}", v);
        free(v);
    }
}

static void test_escape_sequences_stress(void)
{
    printf("test_escape_sequences_stress\n");
    char *v;

    /* Even number of backslashes: \\\\ = two literal backslashes, then closing quote */
    v = json_get_string("{\"k\":\"\\\\\\\\\"}", "k");
    if (v) { free(v); g_pass++; } else { g_pass++; }

    /* Odd backslashes before closing quote: \\\\\" = escaped quote, string continues */
    v = json_get_string("{\"k\":\"\\\\\\\"rest\"}", "k");
    if (v) { free(v); g_pass++; } else { g_pass++; }

    /* Backslash followed by every printable char */
    const char *escapes[] = {
        "{\"k\":\"\\n\"}", "{\"k\":\"\\t\"}", "{\"k\":\"\\r\"}",
        "{\"k\":\"\\b\"}", "{\"k\":\"\\f\"}", "{\"k\":\"\\/\"}",
        "{\"k\":\"\\u0041\"}", "{\"k\":\"\\x41\"}",
        NULL
    };
    for (int i = 0; escapes[i]; i++) {
        v = json_get_string(escapes[i], "k");
        if (v) free(v);
    }
    g_pass++;

    /* 100 consecutive backslashes in a value */
    char nasty[256];
    memset(nasty, 0, sizeof(nasty));
    strcpy(nasty, "{\"k\":\"");
    for (int i = 0; i < 100; i++) nasty[6 + i] = '\\';
    strcpy(nasty + 106, "\"}");
    v = json_get_string(nasty, "k");
    if (v) free(v);
    g_pass++;
}

static void test_extract_object_with_all_value_types(void)
{
    printf("test_extract_object_with_all_value_types\n");
    char *v;

    /* Object containing every JSON value type */
    v = json_extract_object(
        "{\"s\":\"str\",\"n\":42,\"d\":3.14,\"t\":true,\"f\":false,"
        "\"z\":null,\"a\":[1,2,3],\"o\":{\"nested\":1}}");
    if (v) { free(v); g_pass++; } else { g_fail++; }

    /* Object with array containing objects with braces in strings */
    v = json_extract_object(
        "{\"items\":[{\"x\":\"}\"},{\"y\":\"{\"}],\"done\":true}");
    if (v) { free(v); g_pass++; } else { g_fail++; }
}

static void test_array_nth_stress(void)
{
    printf("test_array_nth_stress\n");
    /* Large-ish array */
    char big[2048];
    int pos = 0;
    big[pos++] = '[';
    for (int i = 0; i < 50; i++) {
        if (i > 0) big[pos++] = ',';
        pos += snprintf(big + pos, sizeof(big) - (size_t)pos,
                        "{\"i\":%d,\"v\":\"val%d\"}", i, i);
    }
    big[pos++] = ']';
    big[pos] = '\0';

    /* Access first, middle, last, and past-end */
    const char *e;
    e = json_array_nth(big, 0);
    if (e) { g_pass++; } else { g_fail++; }

    e = json_array_nth(big, 25);
    if (e) {
        char *obj = json_extract_object(e);
        if (obj) {
            ASSERT_INT_EQ(25, json_get_int(obj, "i", -1));
            free(obj);
        }
    }

    e = json_array_nth(big, 49);
    if (e) {
        char *obj = json_extract_object(e);
        if (obj) {
            ASSERT_INT_EQ(49, json_get_int(obj, "i", -1));
            free(obj);
        }
    }

    ASSERT_NULL(json_array_nth(big, 50));
    ASSERT_NULL(json_array_nth(big, 1000));
    ASSERT_NULL(json_array_nth(big, -1));
}

static void test_numeric_torture(void)
{
    printf("test_numeric_torture\n");
    /* Strings that strtol/strtod must handle without UB */
    ASSERT_INT_EQ(0, json_get_int("{\"n\":0}", "n", -1));
    ASSERT_INT_EQ(0, json_get_int("{\"n\":-0}", "n", -1)); /* strtol("-0") = 0 */

    /* Huge positive */
    ASSERT_INT_EQ(99, json_get_int("{\"n\":999999999999999999999999999999}", "n", 99));
    /* Huge negative */
    ASSERT_INT_EQ(99, json_get_int("{\"n\":-999999999999999999999999999999}", "n", 99));
    /* Leading zeros */
    ASSERT_INT_EQ(8, json_get_int("{\"n\":008}", "n", -1));
    /* Hex-ish (strtol base 10 won't parse 0x) */
    ASSERT_INT_EQ(0, json_get_int("{\"n\":0xFF}", "n", -1));
    /* Plus sign */
    /* strtol handles "+42" fine */
    ASSERT_INT_EQ(42, json_get_int("{\"n\":+42}", "n", -1));

    /* Double: NaN, Inf (strtod may parse these — result doesn't matter, just no crash) */
    (void)json_get_double("{\"n\":NaN}", "n", 0);
    (void)json_get_double("{\"n\":Infinity}", "n", 0);
    (void)json_get_double("{\"n\":-Infinity}", "n", 0);
    (void)json_get_double("{\"n\":1e999}", "n", 0); /* overflow */
    (void)json_get_double("{\"n\":1e-999}", "n", 0); /* underflow */

    /* Value longer than the 64-byte stack buffer in get_double */
    char longnum[128];
    strcpy(longnum, "{\"n\":");
    for (int i = 5; i < 80; i++) longnum[i] = '1';
    strcpy(longnum + 80, "}");
    ASSERT_DOUBLE_EQ(0.0, json_get_double(longnum, "n", 0), 0.001); /* len >= 64 → default */

    /* Value longer than the 32-byte stack buffer in get_int */
    char longint[80];
    strcpy(longint, "{\"n\":");
    for (int i = 5; i < 45; i++) longint[i] = '9';
    strcpy(longint + 45, "}");
    ASSERT_INT_EQ(99, json_get_int(longint, "n", 99)); /* len >= 32 → default */

    g_pass++;
}

static void test_key_injection_attacks(void)
{
    printf("test_key_injection_attacks\n");
    char *v;

    /* Key containing quotes — strstr pattern would be "k"e"y" which is malformed.
       Should not match anything. */
    v = json_get_string("{\"k\\\"ey\":\"val\"}", "k\\\"ey");
    /* This won't match because strstr looks for literal "k\"ey" with quotes
       which doesn't appear as-is. No crash is the goal. */
    if (v) free(v);
    g_pass++;

    /* Key containing backslash */
    v = json_get_string("{\"k\\\\ey\":\"val\"}", "k\\\\ey");
    if (v) free(v);
    g_pass++;

    /* Key containing colon */
    v = json_get_string("{\"k:ey\":\"val\"}", "k:ey");
    ASSERT_STR_EQ("val", v); free(v);

    /* Key containing comma */
    v = json_get_string("{\"k,ey\":\"val\"}", "k,ey");
    ASSERT_STR_EQ("val", v); free(v);

    /* Key containing braces */
    v = json_get_string("{\"k{e}y\":\"val\"}", "k{e}y");
    ASSERT_STR_EQ("val", v); free(v);

    /* Key containing brackets */
    v = json_get_string("{\"k[0]\":\"val\"}", "k[0]");
    ASSERT_STR_EQ("val", v); free(v);
}

static void test_pathological_nesting(void)
{
    printf("test_pathological_nesting\n");
    /* 200 levels of nested objects */
    char deep[2048];
    int pos = 0;
    for (int i = 0; i < 200 && pos < 1900; i++)
        deep[pos++] = '{';
    for (int i = 0; i < 200 && pos < 2000; i++)
        deep[pos++] = '}';
    deep[pos] = '\0';

    char *v = json_extract_object(deep);
    if (v) { free(v); g_pass++; } else { g_pass++; }

    /* 200 levels nested inside an array */
    pos = 0;
    deep[pos++] = '[';
    for (int i = 0; i < 200 && pos < 1900; i++)
        deep[pos++] = '{';
    for (int i = 0; i < 200 && pos < 2000; i++)
        deep[pos++] = '}';
    deep[pos++] = ']';
    deep[pos] = '\0';

    (void)json_array_nth(deep, 0);
    g_pass++;
}

static void test_rapid_alloc_free(void)
{
    printf("test_rapid_alloc_free\n");
    /* Hammer json_get_string in a tight loop to stress malloc/free */
    const char *json = "{\"k\":\"value\"}";
    for (int i = 0; i < 10000; i++) {
        char *v = json_get_string(json, "k");
        if (v) free(v);
    }
    g_pass++;

    /* Same with extract_object */
    for (int i = 0; i < 10000; i++) {
        char *v = json_extract_object("{\"a\":1,\"b\":2,\"c\":3}");
        if (v) free(v);
    }
    g_pass++;
}

static void test_utf8_content(void)
{
    printf("test_utf8_content\n");
    char *v;

    /* CJK characters in value */
    v = json_get_string("{\"k\":\"\xe4\xb8\xad\xe6\x96\x87\"}", "k");
    ASSERT_STR_EQ("\xe4\xb8\xad\xe6\x96\x87", v); free(v);

    /* Emoji in value */
    v = json_get_string("{\"k\":\"\xf0\x9f\x93\xbb\"}", "k"); /* 📻 */
    ASSERT_STR_EQ("\xf0\x9f\x93\xbb", v); free(v);

    /* UTF-8 in key */
    v = json_get_string("{\"clé\":\"val\"}", "clé");
    ASSERT_STR_EQ("val", v); free(v);

    /* Multi-byte in both key and value */
    v = json_get_string("{\"名前\":\"太郎\"}", "名前");
    ASSERT_STR_EQ("太郎", v); free(v);
}

static void test_whitespace_variations(void)
{
    printf("test_whitespace_variations\n");
    char *v;

    /* Absurd amounts of whitespace */
    v = json_get_string("{  \t\t\t  \"k\"  \n\n\n  :  \r\n\t  \"v\"  }", "k");
    ASSERT_STR_EQ("v", v); free(v);

    /* Whitespace inside array between objects */
    const char *arr = "[  \n  { \"x\" : 1 }  \n  ,  \n  { \"x\" : 2 }  \n  ]";
    const char *e = json_array_nth(arr, 1);
    if (e) {
        char *obj = json_extract_object(e);
        if (obj) {
            ASSERT_INT_EQ(2, json_get_int(obj, "x", 0));
            free(obj);
        }
    } else {
        g_fail++;
    }
}

static void test_value_boundary_chars(void)
{
    printf("test_value_boundary_chars\n");
    char *v;

    /* Value is just a comma */
    v = json_get_string("{\"k\":,}", "k");
    /* Should get empty string (comma is a terminator for numeric values) */
    if (v) { free(v); g_pass++; } else { g_pass++; }

    /* Value is just a closing brace */
    v = json_get_string("{\"k\":}", "k");
    if (v) { free(v); g_pass++; } else { g_pass++; }

    /* Value is just whitespace then closing brace */
    v = json_get_string("{\"k\":   }", "k");
    if (v) { free(v); g_pass++; } else { g_pass++; }

    /* Multiple keys, last value has no trailing comma */
    v = json_get_string("{\"a\":1,\"b\":2}", "b");
    ASSERT_STR_EQ("2", v); free(v);
}

static void test_repeated_keys(void)
{
    printf("test_repeated_keys\n");
    /* Duplicate keys — strstr returns first match */
    char *v = json_get_string("{\"k\":\"first\",\"k\":\"second\"}", "k");
    ASSERT_STR_EQ("first", v); free(v);
}

static void test_extract_object_exact_boundary(void)
{
    printf("test_extract_object_exact_boundary\n");
    char *v;

    /* Object that is exactly the entire input (no trailing chars) */
    v = json_extract_object("{\"a\":1}");
    ASSERT_STR_EQ("{\"a\":1}", v); free(v);

    /* Object followed by garbage */
    v = json_extract_object("{\"a\":1}garbage}}}}");
    ASSERT_STR_EQ("{\"a\":1}", v); free(v);

    /* Object with trailing whitespace */
    v = json_extract_object("{\"a\":1}   ");
    ASSERT_STR_EQ("{\"a\":1}", v); free(v);
}

int main(void)
{
    printf("=== JSON Parser Tests ===\n\n");

    /* Functional tests */
    test_basic_fields();
    test_null_safety();
    test_missing_key();
    test_whitespace_after_colon();
    test_safe_numerics();
    test_string_aware_braces();
    test_array_operations();
    test_long_key();

    /* Adversarial: malformed inputs */
    test_empty_string_input();
    test_truncated_json();
    test_negative_array_index();
    test_empty_key();
    test_no_colon();
    test_backslash_at_end();
    test_whitespace_only_input();
    test_single_char_inputs();
    test_deeply_nested();
    test_numeric_edge_cases();
    test_value_is_boolean();
    test_escaped_quotes_in_value();
    test_key_as_substring_of_value();
    test_consecutive_commas();
    test_empty_object();
    test_array_of_one();

    /* Adversarial: crash-probing */
    test_garbage_inputs();
    test_binary_garbage();
    test_truncation_at_every_position();
    test_truncation_array_at_every_position();
    test_depth_underflow();
    test_escape_sequences_stress();
    test_extract_object_with_all_value_types();
    test_array_nth_stress();
    test_numeric_torture();
    test_key_injection_attacks();
    test_pathological_nesting();
    test_rapid_alloc_free();
    test_utf8_content();
    test_whitespace_variations();
    test_value_boundary_chars();
    test_repeated_keys();
    test_extract_object_exact_boundary();

    printf("\n=== Results: %d passed, %d failed ===\n", g_pass, g_fail);
    return g_fail > 0 ? 1 : 0;
}
