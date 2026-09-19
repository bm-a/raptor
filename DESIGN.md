# raptor v1 — Design doc (as approved, with amendments, as built)

**Goal:** for one pattern — Rust hands a flat byte buffer (pointer + length) to
C for one call — check at test time that C didn't write past the ends, read
past the ends (read-only), free the buffer, or reuse it after return; print a
plain-language PASS/FAIL report with the failing input.

**Prior art (plain language):** bindgen/cbindgen copy call *shapes*, check
nothing. `cxx` checks *types* for C++, trusts function bodies. Miri checks Rust
but goes blind inside real C ("I stop tracking this memory"). ASan/Valgrind
catch memory bugs if a test triggers them but don't know our contract
(read-only? how long is it valid?). raptor adds: the contract + automatic
harness + plain report, using page-guard/poison/child-isolation techniques.

**Checks as built:** (1) guard-byte overrun/underrun — contiguous
GUARD+buf+GUARD, zero-padding asserted, mismatch=FAIL; (2) free — child process
probe, invalid-free abort=FAIL; (3) hold-and-use-late — poison after return +
forced `c_stash_reuse()`, disturbed poison=FAIL; (4) read-past-length
(read-only) — page-guard (`PROT_NONE`) fault=FAIL; contiguous honestly reports
its blindness (suite asserts guards PASS on pure reads).

**Won't catch:** corruption far from the buffer; pure over-reads on writable
buffers; nested pointers; races/threads; leaks; inputs never fuzzed; C++ or any
second pattern; late-use never exercised (amendment #5).

**Proof:** 6 C functions (1 good + 5 buggy: overrun, underrun, free, overread,
stash), 100 fuzzed sizes/contents each incl. edge sizes 0,1,2,16,17,4095,4096;
buggy must be caught every time with seed printed, good must pass every time.
Reproduce: `cargo test` (14 tests) and
`raptor check --func <name> --fuzz 100` per function.

**Amendments locked:** (i) trade-off #5 wording on late-use; (ii) guards proven
zero-padding + page layout; (iii) page dual-layout = exactly the 7 sizes above
for overrun/underrun, page-always for over-read (#4); (iv) tool renamed
`ffi-guard`→`raptor` (folder too), scope unchanged, no new language in v1.
