# What raptor does NOT catch (honest limits)

- A C function that corrupts **unrelated memory** far from the buffer (outside
  the guards / guard page).
- Pure over-reads on **writable** buffers — indistinguishable from legal reads
  without hardware watchpoints.
- Bugs inside **nested pointers** (a buffer containing another pointer). Only
  the flat outer buffer is checked.
- **Data races / threading** bugs where C shares the pointer with another thread.
- **Leaks**, or misbehavior only on inputs never fuzzed.
- **Late-use the tests never trigger** — detection only catches stashes the
  suite actually reuses (dynamic testing can't prove absence of an
  untriggered bug).
- **Anything beyond the one pattern**: no C++, no other FFI pairs, no second
  buffer pattern in v1.
- **Production protection**: checks run in debug/test builds only.
