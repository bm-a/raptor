#ifndef BUGGY_H
#define BUGGY_H

#include <stddef.h>
#include <stdint.h>

// All functions share the one v1 pattern: (pointer + length), valid for the call only.
// Step 1 scope: only overrun/underrun are checked. The other three are present so the
// test suite can show they are NOT yet caught (honest NOT CHECKED status).

// Correct: writes only inside [buf, buf+len). Safe for any len (len==0: no-op).
void c_good(uint8_t *buf, size_t len);

// Bug 1: writes exactly 1 byte past the end: buf[len] = 0xFF (for len==0, buf[0]).
void c_overrun(uint8_t *buf, size_t len);

// Bug 1b: writes exactly 1 byte before the start: buf[-1] = 0xFF (for len==0, buf[-1]).
void c_underrun(uint8_t *buf, size_t len);

// --- Step 2/3 functions (fully checked since build 2) ---
void c_free_it(uint8_t *buf, size_t len);      // frees the buffer (must NOT)
void c_overread(const uint8_t *buf, size_t len); // reads buf[len] (must NOT, read-only)
void c_stash(uint8_t *buf, size_t len);        // saves pointer for late use (must NOT)
uint8_t c_stash_reuse(void);                   // uses the stashed pointer late
void c_stash_reset(void);                      // clears the stashed pointer (harness hygiene)

#endif
