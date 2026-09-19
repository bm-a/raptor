# raptor wiki — Home

**raptor** is a test-time checker for one dangerous pattern at the Rust-to-C
FFI boundary: **Rust hands a flat byte buffer (pointer + length) to a C
function for the duration of one call.**

It verifies, at test time, that the C function does not:

1. write past the ends of the buffer (overrun / underrun),
2. free the buffer,
3. save the pointer and use it after return (hold-and-use-late),
4. read past the end of a read-only buffer (over-read).

… and prints a **plain-language report** (which checks passed/failed, and the
exact failing input) that a non-programmer can hand to someone else.

- New here? Start with [[What raptor checks]].
- Want to reproduce results yourself? See [[How to verify]].
- Want the honest limits? See [[What raptor does NOT catch]] and [[Decisions log]].
- Reference: [[CLI reference]] · [[The buggy test suite]] · [[Prior art]] · [[FAQ]]

**Status: v1 complete.** All 14 tests green; every bug type caught 100/100
fuzzed runs; zero false positives on the correct function.
`raptor` is a test harness, not a new language, and v1 will not become one.
