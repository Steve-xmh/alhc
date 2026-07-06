/// Non-variadic wrappers for curl_easy_setopt.
///
/// On ARM64 (macOS), calling a variadic C function through a Rust
/// non-variadic function pointer does not set up the variadic save
/// area, causing `va_arg` inside curl to read garbage.
///
/// These thin C wrappers let Rust call into curl safely. The C
/// compiler generates the correct variadic call sequence.
///
/// Only a handful of fundamental types are needed (string, long,
/// pointer/function, no-arg perform) — curl option constants are
/// hardcoded from curl/curl.h (they are part of the stable ABI).

#include <stddef.h>

// Declared by libcurl (resolved at link time from system library).
extern int curl_easy_setopt(void *curl, int option, ...);
extern void *curl_easy_init(void);
extern int curl_easy_perform(void *curl);
extern int curl_easy_getinfo(void *curl, int info, long *val);
extern void curl_easy_cleanup(void *curl);
extern const char *curl_easy_strerror(int code);

// ---- Non-variadic wrappers ----

void *alhc_easy_init(void) {
    return curl_easy_init();
}

int alhc_setopt_str(void *h, int opt, const char *s) {
    return curl_easy_setopt(h, opt, s);
}

int alhc_setopt_long(void *h, int opt, long v) {
    return curl_easy_setopt(h, opt, v);
}

int alhc_setopt_ptr(void *h, int opt, void *p) {
    return curl_easy_setopt(h, opt, p);
}

int alhc_easy_perform(void *h) {
    return curl_easy_perform(h);
}

int alhc_easy_getinfo_code(void *h, long *val) {
    return curl_easy_getinfo(h, 0x200002, val); // CURLINFO_RESPONSE_CODE
}

void alhc_easy_cleanup(void *h) {
    curl_easy_cleanup(h);
}

const char *alhc_easy_strerror(int code) {
    return curl_easy_strerror(code);
}
