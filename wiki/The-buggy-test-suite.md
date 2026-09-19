# The buggy test suite (why you can trust the tool)

The suite **deliberately includes buggy C functions** (`c_src/buggy.c`) so the
tool must prove it catches failures instead of always reporting "safe":

| C function | bug | caught by |
|---|---|---|
| `c_good` | none (writes `0x42` inside `[buf, buf+len)`) | must PASS everywhere, 0 false alarms |
| `c_overrun` | writes `buf[len]` (1 past the end) | trailing guard / page fault |
| `c_underrun` | writes `buf[-1]` (1 before the start) | leading guard |
| `c_free_it` | `free(buf)` — buffer owned by Rust | child abort (invalid free) |
| `c_overread` | reads `buf[len]` on a read-only buffer | page-guard fault (contiguous guards honestly PASS — asserted as a gap demo) |
| `c_stash` + `c_stash_reuse` | saves pointer, writes `0xEE` late | poison disturbed after forced reuse |

Each buggy function is fuzzed with **100 runs** of varied buffer sizes and
content (including locked edge sizes `0, 1, 2, 16, 17, 4095, 4096`), with
deterministic xorshift content so every failure reproduces from `(len, seed)`.

**Success bar:** zero misses per bug type, zero false positives on `c_good`
across the same fuzzing. Current status: met — see [[How to verify]].

Hygiene details: `c_stash_reset()` clears C-side stash globals between runs;
free/page probes run in child processes so aborts and faults never crash the
runner; buffer content is deterministic from seed so reports are reproducible.
