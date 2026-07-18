/// Typed, non-variadic wrappers around the small libcurl API surface used by
/// ALHC. Keeping the variadic calls in C avoids ABI issues on ARM64 while the
/// primitive interface below avoids requiring curl development headers.

#include <limits.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

struct curl_slist {
    char *data;
    struct curl_slist *next;
};

struct alhc_curl_waitfd {
    int fd;
    short events;
    short revents;
};

union alhc_curl_msg_data {
    void *whatever;
    int result;
};

struct alhc_curl_msg {
    int msg;
    void *easy_handle;
    union alhc_curl_msg_data data;
};

typedef size_t (*alhc_write_callback)(char *, size_t, size_t, void *);

extern int curl_global_init(long flags);
extern void *curl_easy_init(void);
extern void curl_easy_cleanup(void *easy);
extern int curl_easy_setopt(void *easy, int option, ...);
extern int curl_easy_getinfo(void *easy, int info, ...);
extern const char *curl_easy_strerror(int code);
extern struct curl_slist *curl_slist_append(struct curl_slist *list, const char *data);
extern void curl_slist_free_all(struct curl_slist *list);

extern void *curl_multi_init(void);
extern int curl_multi_cleanup(void *multi);
extern int curl_multi_add_handle(void *multi, void *easy);
extern int curl_multi_remove_handle(void *multi, void *easy);
extern int curl_multi_perform(void *multi, int *running_handles);
extern int curl_multi_timeout(void *multi, long *timeout_ms);
extern int curl_multi_wait(
    void *multi,
    struct alhc_curl_waitfd extra_fds[],
    unsigned int extra_nfds,
    int timeout_ms,
    int *numfds);
extern struct alhc_curl_msg *curl_multi_info_read(void *multi, int *msgs_in_queue);
extern const char *curl_multi_strerror(int code);

// Stable libcurl ABI constants used by the wrappers.
#define ALHC_CURL_GLOBAL_DEFAULT 3L
#define ALHC_CURLE_OUT_OF_MEMORY 27
#define ALHC_CURLMSG_DONE 1
#define ALHC_CURL_WAIT_POLLIN 1

#define ALHC_CURLOPT_WRITEDATA 10001
#define ALHC_CURLOPT_URL 10002
#define ALHC_CURLOPT_WRITEFUNCTION 20011
#define ALHC_CURLOPT_POSTFIELDS 10015
#define ALHC_CURLOPT_HTTPHEADER 10023
#define ALHC_CURLOPT_HEADERDATA 10029
#define ALHC_CURLOPT_NOBODY 44
#define ALHC_CURLOPT_FOLLOWLOCATION 52
#define ALHC_CURLOPT_MAXREDIRS 68
#define ALHC_CURLOPT_HEADERFUNCTION 20079
#define ALHC_CURLOPT_NOSIGNAL 99
#define ALHC_CURLOPT_CUSTOMREQUEST 10036
#define ALHC_CURLOPT_ACCEPT_ENCODING 10102
#define ALHC_CURLOPT_PRIVATE 10103
#define ALHC_CURLOPT_TIMEOUT_MS 155
#define ALHC_CURLOPT_CONNECTTIMEOUT_MS 156
#define ALHC_CURLOPT_POSTFIELDSIZE_LARGE 30120
#define ALHC_CURLINFO_RESPONSE_CODE 0x200002
#define ALHC_CURLINFO_PRIVATE 0x100015

int alhc_global_init(void) {
    return curl_global_init(ALHC_CURL_GLOBAL_DEFAULT);
}

void *alhc_easy_init(void) {
    return curl_easy_init();
}

void alhc_easy_cleanup(void *easy) {
    curl_easy_cleanup(easy);
}

int alhc_easy_configure(
    void *easy,
    const char *url,
    const char *method,
    void *userdata,
    size_t slot,
    alhc_write_callback write_callback,
    alhc_write_callback header_callback) {
    int code;

#define ALHC_SETOPT(option, value)                  \
    do {                                             \
        code = curl_easy_setopt(easy, option, value); \
        if (code != 0) return code;                 \
    } while (0)

    ALHC_SETOPT(ALHC_CURLOPT_URL, url);
    ALHC_SETOPT(ALHC_CURLOPT_CUSTOMREQUEST, method);
    ALHC_SETOPT(ALHC_CURLOPT_FOLLOWLOCATION, 1L);
    ALHC_SETOPT(ALHC_CURLOPT_MAXREDIRS, 10L);
    ALHC_SETOPT(ALHC_CURLOPT_TIMEOUT_MS, 30000L);
    ALHC_SETOPT(ALHC_CURLOPT_CONNECTTIMEOUT_MS, 10000L);
    ALHC_SETOPT(ALHC_CURLOPT_NOSIGNAL, 1L);
    ALHC_SETOPT(ALHC_CURLOPT_ACCEPT_ENCODING, "");
    ALHC_SETOPT(ALHC_CURLOPT_WRITEFUNCTION, write_callback);
    ALHC_SETOPT(ALHC_CURLOPT_WRITEDATA, userdata);
    ALHC_SETOPT(ALHC_CURLOPT_HEADERFUNCTION, header_callback);
    ALHC_SETOPT(ALHC_CURLOPT_HEADERDATA, userdata);
    ALHC_SETOPT(ALHC_CURLOPT_PRIVATE, (void *)(uintptr_t)(slot + 1));

    if (strcmp(method, "HEAD") == 0) {
        ALHC_SETOPT(ALHC_CURLOPT_NOBODY, 1L);
    }

#undef ALHC_SETOPT
    return 0;
}

int alhc_easy_set_body(void *easy, const unsigned char *data, size_t len) {
    int code;
    if (len > (size_t)LLONG_MAX) {
        return ALHC_CURLE_OUT_OF_MEMORY;
    }

    code = curl_easy_setopt(easy, ALHC_CURLOPT_POSTFIELDS, data);
    if (code != 0) {
        return code;
    }
    return curl_easy_setopt(
        easy,
        ALHC_CURLOPT_POSTFIELDSIZE_LARGE,
        (long long)len);
}

int alhc_easy_set_headers(
    void *easy,
    const char *const *headers,
    size_t count,
    void **list_out) {
    struct curl_slist *list = NULL;
    size_t index;

    *list_out = NULL;
    for (index = 0; index < count; index++) {
        struct curl_slist *next = curl_slist_append(list, headers[index]);
        if (next == NULL) {
            curl_slist_free_all(list);
            return ALHC_CURLE_OUT_OF_MEMORY;
        }
        list = next;
    }

    if (list != NULL) {
        int code = curl_easy_setopt(easy, ALHC_CURLOPT_HTTPHEADER, list);
        if (code != 0) {
            curl_slist_free_all(list);
            return code;
        }
    }

    *list_out = list;
    return 0;
}

void alhc_slist_free(void *list) {
    if (list != NULL) {
        curl_slist_free_all((struct curl_slist *)list);
    }
}

int alhc_easy_getinfo_code(void *easy, long *value) {
    return curl_easy_getinfo(easy, ALHC_CURLINFO_RESPONSE_CODE, value);
}

const char *alhc_easy_strerror(int code) {
    return curl_easy_strerror(code);
}

void *alhc_multi_init(void) {
    return curl_multi_init();
}

int alhc_multi_cleanup(void *multi) {
    return curl_multi_cleanup(multi);
}

int alhc_multi_add(void *multi, void *easy) {
    return curl_multi_add_handle(multi, easy);
}

int alhc_multi_remove(void *multi, void *easy) {
    return curl_multi_remove_handle(multi, easy);
}

int alhc_multi_perform(void *multi, int *running) {
    return curl_multi_perform(multi, running);
}

int alhc_multi_timeout_ms(void *multi, int *timeout_ms) {
    long timeout = -1;
    int code = curl_multi_timeout(multi, &timeout);
    if (code != 0) {
        return code;
    }
    if (timeout < 0 || timeout > 1000) {
        timeout = 1000;
    }
    *timeout_ms = (int)timeout;
    return 0;
}

int alhc_multi_wait_fd(void *multi, int command_fd, int timeout_ms, int *numfds) {
    struct alhc_curl_waitfd waitfd;
    waitfd.fd = command_fd;
    waitfd.events = ALHC_CURL_WAIT_POLLIN;
    waitfd.revents = 0;
    return curl_multi_wait(multi, &waitfd, 1, timeout_ms, numfds);
}

int alhc_multi_next_done(
    void *multi,
    void **easy_out,
    int *result_out,
    size_t *slot_out) {
    int remaining = 0;
    struct alhc_curl_msg *message;

    while ((message = curl_multi_info_read(multi, &remaining)) != NULL) {
        if (message->msg == ALHC_CURLMSG_DONE) {
            void *private_data = NULL;
            *easy_out = message->easy_handle;
            *result_out = message->data.result;
            if (curl_easy_getinfo(
                    message->easy_handle,
                    ALHC_CURLINFO_PRIVATE,
                    &private_data) != 0 || private_data == NULL) {
                *slot_out = (size_t)-1;
            } else {
                *slot_out = (size_t)(uintptr_t)private_data - 1;
            }
            return 1;
        }
    }
    return 0;
}

const char *alhc_multi_strerror(int code) {
    return curl_multi_strerror(code);
}
