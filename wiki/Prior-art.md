# Prior art (what exists, and what raptor adds)

- **bindgen**: reads a C header, auto-writes the matching Rust declaration.
  Copies the *shape* of the call; checks nothing about what C *does* with the
  pointer.
- **cbindgen**: the reverse (Rust → C header). Same limit: glue, no behavior
  check.
- **`cxx` crate**: safer glue for Rust↔C++ (not C). Checks *types* at compile
  time so many mix-ups are impossible — but trusts function bodies completely.
  A C++ function writing past a buffer still passes `cxx`.
- **Miri** (Rust's UB detector): catches memory misuse *inside Rust*. On real
  C calls it refuses ("can't call foreign function") or, in experimental mode,
  explicitly stops tracking the shared memory. Blind inside C either way.
- **AddressSanitizer / Valgrind**: excellent general C bug-catchers *if* a test
  triggers the bug — but they don't know your contract (read-only? valid how
  long? how long is the buffer?). No per-contract PASS/FAIL + failing-input
  report tied to the Rust signature.

**What raptor adds:** it starts from the contract (this pointer, this length,
read-only or not, valid only until return), builds the test harness around it
automatically, and reports per-check PASS/FAIL with the failing input — using
ASan-style techniques (canaries, poisoning, guard pages, child isolation)
under the hood. It complements the tools above; it replaces none of them.
