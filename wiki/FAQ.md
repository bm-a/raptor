# FAQ

**Does passing mean my C code is safe?**
No. It means these four bug types didn't trigger on the tested inputs. See
[[What raptor does NOT catch]].

**Why does the over-read check need a special layout?**
Reads don't modify memory, so canary bytes can't see them. The page-guard
layout makes out-of-bounds reads fault. The suite contains a test proving the
contiguous layout passes an over-read — the gap is demonstrated, not hidden.

**Why do buggy-function CLI runs exit with code 1?**
Exit 1 means "UNSAFE / bug caught" — the harness working correctly. In fuzz
mode the summary instead reports `PASS — bug caught on every run` with exit 0.

**Can I use raptor on my own C function?**
v1 ships with the built-in buggy suite as the proof vehicle; the signature
checker already accepts any flat-buffer `(pointer + length)` pair and rejects
anything else. Pointing the harness at arbitrary user C code is proposed
future work, not v1 scope.

**Will raptor become a language / general verifier?**
Not in v1. One pattern, Rust-and-C only. Extensions need explicit approval.

**How do I report a problem?**
Open an issue with the full FAIL block (function, length, seed, content,
reproduce command) — everything needed is printed; no source reading required.
