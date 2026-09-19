# DECISIONS.md — running plain-language log of completeness-vs-tractability trade-offs

1. **Flat buffers only, no nested pointers.** Checking pointers-inside-buffers
   would need full memory tracking. v1 checks the outer flat buffer only.
2. **Test/debug builds only.** Zero release overhead — but no production
   protection. This is a test harness, not a runtime shield.
3. **Free detection needs the child-process trick, not an allocator shim.**
   Original plan said "allocator shim with a flag"; what we built is simpler
   and more general: C only gets an *interior* pointer, so any `free()` it
   attempts aborts the child process, which the parent counts as caught. No
   cooperation from the C code needed. Won't distinguish "free" from "heap
   corruption that crashes" — both are reported as freed-or-crashed, honestly.
4. **Read-past-length needs the page guard, not (only) ASan.** Pure reads leave
   no trace in guard bytes (the suite proves this with a test that PASSES
   guards on an over-read). The page-guard layout (`PROT_NONE` page right after
   the buffer) turns the over-read into a child fault. ASan would also work but
   needs an instrumented build; the page guard needs only mmap/mprotect.
   Over-read verdicts therefore come from the page layout for EVERY input size
   (the page technique works for any size) — while the locked-7 dual-layout
   promise (#6) governs overrun/underrun only.
5. **Late-use detection only catches bugs the tests actually exercise.** A C
   function that stashes the pointer and never touches it again during our
   specific tests won't be flagged. Inherent to dynamic testing — we can't
   prove the absence of a bug we never triggered. Our suite forces a second
   access (poison + `c_stash_reuse()`), and `c_stash_reset()` keeps runs
   isolated (C global state would otherwise leak between tests in one process).
6. **Page-boundary dual layout is a closed list, not a catch-all.** Exactly the
   7 sizes `0, 1, 2, 16, 17, 4095, 4096` run in both layouts for
   overrun/underrun; every other fuzz size runs contiguous-guards only. Adding
   a size needs explicit approval. (Over-read always uses the page layout —
   see #4 — because contiguous cannot see reads at all.)
7. **Portability lesson: `getpagesize()`, not `sysconf(_SC_PAGESIZE)`.** The
   sysconf constant differs per libc (30 on glibc, 39 on Bionic); the wrong
   constant silently returned 1 on our test machine, which made every page
   probe fail misleadingly. We call `getpagesize()` (verified 4096 on-device),
   and the suite asserts page placement per run instead of trusting constants.
8. **Name: `ffi-guard` → `raptor` (cosmetic only).** Same one-pattern scope,
   same CLI shape (`raptor check ...`). Folder renamed to match.
9. **Scope guard: `raptor` will not become a new language in v1.** Proposed
   and declined: v1 stays a narrow Rust-and-C test harness for the single
   buffer pattern. Any such evolution needs renegotiated scope, not silent
   expansion.
10. **Two real bugs found by building (not by design):** (a) the
    `sysconf(30)==1` portability trap (#7); (b) the CLI silently printed
    nothing for overrun/underrun on locked sizes (a missing `print!` before an
    early `return` — every other branch had one). Both are now covered by the
    suite/CLI checks, which is exactly why every claim needs a runnable check.
