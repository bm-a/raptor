# What raptor checks (and how each check works)

All checks run **only in debug/test builds**, never in production. For each
test call the harness allocates `GUARD + buffer + GUARD` (16 bytes of known
pattern `0xAA` before, `0xBB` after), passes only the middle pointer to C, and
keeps a private copy of the original buffer.

## 1. Overrun / underrun (C writes past the length)

After C returns, the harness compares the guard bytes to the known pattern.
A write past the end (or before the start) must touch a guard → mismatch =
FAIL. C can't know to preserve the canaries.

Additionally, an **asserted zero-padding invariant** (`guard == buffer ± 16`,
checked every run) guarantees a 1-byte overrun always touches a guard — a
small overrun can't hide in allocator padding.

## 2. Free (C frees a buffer it doesn't own)

C only ever receives an **interior pointer** (the middle of our allocation),
so *any* `free(buf)` it attempts is an invalid free that aborts the process.
The call runs in a **child process** (`raptor __free-probe`): a dead child
(killed by signal, e.g. SIGABRT) = FAIL "freed the buffer"; a child that exits
0 and frees our allocation cleanly = PASS. No cooperation from the C code is
needed — the trick works on any C function.

## 3. Hold-and-use-late (C saves the pointer, uses it after return)

Immediately after C returns, the harness **poisons** the whole allocation
(fills it with `0xFE`), then forces a second access via `c_stash_reuse()`.
If C saved the pointer, the reuse writes the marker `0xEE` into the poison =
FAIL. A correct C function stashes nothing, so the reuse is a no-op and the
poison stays intact = PASS. `c_stash_reset()` runs before/after each case so
C-side global state can't leak between tests in one process.

> Inherent limit (see [[What raptor does NOT catch]]): only stashes the suite
> actually reuses are caught — dynamic testing can't prove the absence of a
> bug it never triggers.

## 4. Read-past-length on read-only buffers

Pure reads leave **no trace** in guard bytes — the suite proves this with a
test asserting guards PASS on an over-read. So for reads the harness uses a
**page-guard layout**: the buffer is placed to end exactly at a protected
`PROT_NONE` memory page (via `mmap`/`mprotect`). Reading `buf[len]` faults the
child process = FAIL caught. Code that only reads inside the buffer survives =
PASS. (AddressSanitizer would also work but needs an instrumented build; the
page guard needs only `mmap`/`mprotect`.)

## Layouts

- **Contiguous guards** (all fuzz sizes): one `GUARD + buffer + GUARD` block,
  zero padding asserted.
- **Page guard** (dual-run for exactly the 7 locked sizes `0, 1, 2, 16, 17,
  4095, 4096` for overrun/underrun; always used for over-read verdicts, since
  contiguous layout is blind to reads).
