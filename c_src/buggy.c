#include "buggy.h"
#include <stdlib.h>

void c_good(uint8_t *buf, size_t len) {
    for (size_t i = 0; i < len; i++) {
        buf[i] = 0x42;
    }
}

void c_overrun(uint8_t *buf, size_t len) {
    // Deliberate 1-byte overrun. Guard bytes (Step 1) must catch this.
    buf[len] = 0xFF;
}

void c_underrun(uint8_t *buf, size_t len) {
    (void)len;
    // Deliberate 1-byte underrun. Guard bytes before the buffer must catch this.
    buf[-1] = 0xFF;
}

// ---- Steps 2/3: deliberately buggy, caught by the harness ----
static uint8_t *stashed = 0;
static size_t stashed_len = 0;

void c_stash_reset(void) {
    stashed = 0;
    stashed_len = 0;
}

void c_free_it(uint8_t *buf, size_t len) {
    (void)len;
    free(buf); // BUG: buffer is owned by Rust, C must not free it
}

void c_overread(const uint8_t *buf, size_t len) {
    volatile uint8_t sink = buf[len]; // BUG: read 1 past a read-only buffer
    (void)sink;
}

void c_stash(uint8_t *buf, size_t len) {
    stashed = buf; // BUG: hold pointer past return
    stashed_len = len;
}

uint8_t c_stash_reuse(void) {
    if (stashed == 0) return 0;
    stashed[0] = 0xEE; // late use through the stashed pointer
    (void)stashed_len;
    return stashed[0];
}
