# Decisions log (completeness vs tractability)

Running plain-language log — every entry trades completeness for something we
could actually build and test.

1. **Flat buffers only.** Nested pointers would need full memory tracking.
2. **Test/debug builds only.** Zero release overhead, no production protection.
3. **Free detection via child process, not allocator shim.** C gets an interior
   pointer, so any `free()` aborts the child = caught, with no C cooperation
   needed. Can't distinguish "free" from "heap corruption that crashes" —
   both report as freed-or-crashed.
4. **Over-read via page guard, not (only) ASan.** Reads leave no trace in
   canaries (suite proves it); `PROT_NONE` page turns over-reads into child
   faults. Page verdicts apply to every input size; the locked-7 dual-layout
   rule governs overrun/underrun.
5. **Late-use only catches exercised bugs.** A stash never reused during our
   tests is invisible — inherent to dynamic testing, stated plainly.
6. **Page dual-layout = closed list** (`0, 1, 2, 16, 17, 4095, 4096`) for
   overrun/underrun. Adding a size needs approval.
7. **Portability: `getpagesize()`, not `sysconf(_SC_PAGESIZE)`.** The sysconf
   constant differs per libc (30 glibc vs 39 Bionic); the wrong one returned 1
   on our machine. Suite asserts placement per run.
8. **Name `ffi-guard` → `raptor`**, cosmetic only; folder renamed to match.
9. **Scope guard: no new language in v1.** Proposed, declined. v1 is a narrow
   Rust-and-C harness for one pattern; evolution needs renegotiated scope.
10. **Two real bugs found while building** (missing `print!` silencing two CLI
    paths; the sysconf trap) — both now covered by checks. The verifiability
    loop working as designed.
