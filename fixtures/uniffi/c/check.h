/* Minimal checks for the C ports of uniffi's fixture tests. */
#ifndef UNIFFI_FIXTURE_CHECK_H
#define UNIFFI_FIXTURE_CHECK_H

#include <stdio.h>
#include <string.h>

#include "uniffi_runtime.h"

static int check_failures = 0;
static int check_count = 0;

#define CHECK(cond)                                                                   \
    do {                                                                              \
        ++check_count;                                                                \
        if (!(cond)) {                                                                \
            ++check_failures;                                                         \
            fprintf(stderr, "%s:%d: CHECK failed: %s\n", __FILE__, __LINE__, #cond);  \
        }                                                                             \
    } while (0)

/* Whether `s` holds exactly `expected`. */
static inline bool check_str(uniffi_string s, const char *expected) {
    return s.len == strlen(expected) && (s.len == 0 || memcmp(s.data, expected, s.len) == 0);
}

/* Like check_str, then frees `s`. */
static inline bool check_take_str(uniffi_string s, const char *expected) {
    bool ok = check_str(s, expected);
    if (!ok) {
        fprintf(stderr, "  got \"%.*s\", expected \"%s\"\n", (int)s.len, s.data ? s.data : "", expected);
    }
    uniffi_string_free(&s);
    return ok;
}

/* Prints the summary; the process exit code. */
static inline int check_report(void) {
    printf("%d checks, %d failures\n", check_count, check_failures);
    return check_failures == 0 ? 0 : 1;
}

#endif
