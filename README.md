# raptor — plain-language README

**What this is, in one sentence:** a test-time checker for one dangerous pattern —
"Rust hands a flat byte buffer (pointer + length) to a C function for one call."
It catches C writing past the ends, freeing the buffer, saving the pointer and
using it late, and reading past the end of a read-only buffer. Test/debug builds
only — never in production.

## What it checks (all four, every run reports each relevant one)
1. **Write-before-start / write-past-end** — 16 guard bytes sit flush against
   both ends of the buffer (zero padding asserted every run). Any change = FAIL.
2. **Free** — the call runs in a child process. C only ever receives an
   *interior* pointer, so any `free()` it attempts is an invalid free that kills
   the child = FAIL ("killed by signal"). A correct function leaves the child
   alive = PASS.
3. **Hold-and-use-late** — right after C returns we poison the whole allocation
   (`0xFE`), then force a second access (`c_stash_reuse()`). If C saved the
   pointer, the reuse writes `0xEE` into the poison = FAIL. Correct code stashes
   nothing, poison stays intact = PASS.
4. **Read-past-length (read-only)** — plain guard bytes *cannot* catch pure
   reads (reads leave no trace; the suite proves this gap with a passing-guards
   test). So the buffer is placed to end exactly at a protected memory page
   (`PROT_NONE`): reading `buf[len]` faults the child = FAIL caught. Correct
   code only reads inside = PASS.

For 7 locked sizes — `0, 1, 2, 16, 17, 4095, 4096` — overrun/underrun run in
BOTH layouts (contiguous guards + page guard). Over-read verdicts always come
from the page layout (the only layout that can see reads). No other pattern,
no other language, no C++ — v1 stays narrow on purpose. **raptor is a test
harness, not a new language, and will not become one in v1.**

## What it does NOT catch (honest list)
- Corruption of unrelated memory far from the buffer.
- Pure over-reads on *writable* buffers (indistinguishable from legal reads).
- Nested pointers (only the flat outer buffer is checked), threads/races,
  leaks, inputs never fuzzed, C++.
- Late-use bugs the tests never trigger: detection only catches stashes the
  suite actually reuses (see DECISIONS.md #5).

## How to verify without reading code (copy-paste, from `~/raptor`)

Full suite — expect **14 passed, 0 failed**:
```sh
cd ~/raptor
cargo test
```
You will see lines like:
```
overrun: 100 runs, 0 misses
underrun: 100 runs, 0 misses
free-it: 100 runs, 0 misses
stash: 100 runs, 0 misses
overread/page: 100 runs, 0 misses
good: 100 runs, 0 false positives
```

Fuzz each function through the CLI (100 varied sizes each) — expect 6× PASS:
```sh
R='extern "C" fn f(buf: *mut u8, len: usize)'; C='void f(uint8_t *buf, size_t len)'
for f in good overrun underrun free overread stash; do
  ./target/debug/raptor check --rust-sig "$R" --c-sig "$C" --func $f --fuzz 100
done
# good → "Result: PASS — zero false positives."
# each buggy one → "Result: PASS — bug caught on every run."
```

Single cases (every FAIL prints length + seed + content + reproduce command):
```sh
./target/debug/raptor check --rust-sig "$R" --c-sig "$C" --func overrun --len 16 --seed 7
# → [FAIL] Write-past-end … trailing guard byte 0 was changed … Result: UNSAFE (exit 1)
./target/debug/raptor check --rust-sig "$R" --c-sig "$C" --func good --len 16 --seed 7
# → all [PASS] … Result: SAFE on this input (exit 0)
```

Real output from this machine (all six singles):
```
good     exit=0 : Result: SAFE on this input.
overrun  exit=1 : Result: UNSAFE — caught on this input (reproduce: raptor check --func c_overrun --len 16 --seed 7).
underrun exit=1 : Result: UNSAFE — caught on this input (reproduce: raptor check --func c_underrun --len 16 --seed 7).
free     exit=1 : Result: UNSAFE — freed the buffer (reproduce: raptor check --func c_free_it --len 16 --seed 7).
overread exit=1 : Result: UNSAFE — over-read caught (reproduce: raptor check --func c_overread --len 16 --seed 7).
stash    exit=1 : Result: UNSAFE — late use caught (reproduce: raptor check --func c_stash --len 16 --seed 7).
```
Hand any FAIL block to someone else — no source reading needed.
