# raptor — Runtime Buffer-Safety Checks at the Rust-to-C FFI Boundary

> **Rust FFI memory-safety testing tool:** catches C buffer overruns, buffer
> over-reads, invalid `free()`, and hold-and-use-late (use-after-return) bugs
> where Rust passes a byte buffer (pointer + length) to C — with guard
> canaries, memory poisoning, page guards, and plain-language reports.
> Test/debug builds only. No dependencies. 14 tests green.

**Links:** [Repository](https://github.com/bm-a/raptor) ·
[Wiki](https://github.com/bm-a/raptor/wiki) ·
[How to verify](https://github.com/bm-a/raptor/wiki/How-to-verify) ·
[What it does NOT catch](https://github.com/bm-a/raptor/wiki/What-raptor-does-NOT-catch) ·
[AI-agent summary](llms.txt)

**Keywords:** rust, ffi, foreign function interface, memory safety, buffer
overflow detection, buffer overrun, use-after-free, sanitizer testing, fuzzing,
c interop, unsafe rust, guard canary, address sanitizer alternative.

---

## Table of contents

1. [The problem](#1-the-problem)
2. [What raptor does](#2-what-raptor-does)
3. [The four checks (how each works)](#3-the-four-checks-how-each-works)
4. [Quickstart](#4-quickstart)
5. [CLI reference](#5-cli-reference)
6. [Understanding the report](#6-understanding-the-report)
7. [Test suite & proof](#7-test-suite--proof)
8. [Project structure](#8-project-structure)
9. [How raptor compares to existing tools](#9-how-raptor-compares-to-existing-tools)
10. [Limitations (honest list)](#10-limitations-honest-list)
11. [Scope & roadmap](#11-scope--roadmap)
12. [For AI agents](#12-for-ai-agents)
13. [Contributing](#13-contributing)
14. [License](#14-license)

---

## 1. The problem

When safe Rust calls into C (or C calls into Rust), the Rust compiler's safety
guarantees stop at the boundary. Rust's borrow checker cannot see what a C
function does with a pointer it receives: whether it frees it, holds onto it
past its valid lifetime, writes past the given length, or reads past it.
Existing glue generators (`bindgen`, `cbindgen`, the `cxx` crate) emit the
*calling convention* but verify nothing about what the C side *does*.
Miri (Rust's undefined-behavior detector) goes blind inside real C code.
AddressSanitizer and Valgrind catch memory bugs only if a test happens to
trigger them — and they don't know your contract (read-only? valid how long?).

**raptor narrows that verification gap for one specific, common, dangerous
pattern:** a Rust function passes a flat buffer (pointer + length) to a C
function for the duration of one call. It automatically builds a test harness
around that contract and reports, in plain language, whether the C side upheld it.

## 2. What raptor does

Given a Rust FFI signature (`extern "C" fn`) and the matching C signature,
raptor:

1. **Generates an instrumented call** — allocates `GUARD + buffer + GUARD`,
   passes only the middle pointer to C, and applies runtime checks in
   debug/test builds (zero release impact — it simply isn't present there).
2. **Checks four buffer-safety properties** (details in §3): no
   write-before-start, no write-past-end, no `free()` of the buffer, no
   hold-and-use-late, no read-past-length on read-only buffers.
3. **Prints a plain-language report** — which properties passed/failed, and on
   failure the exact input (length, seed, content preview, reproduce command)
   in a form you can hand to someone else without reading source code.

## 3. The four checks (how each works)

### 3a. Overrun / underrun — guard canaries

16 known-pattern bytes (`0xAA` before, `0xBB` after) sit flush against both
buffer ends. After C returns, any change = FAIL. A per-run asserted invariant
(`guard == buffer ± 16`, zero padding by construction) guarantees even a
1-byte overrun touches a guard — nothing can hide in allocator padding.

### 3b. Free — child-process invalid-free trap

C only ever receives an *interior* pointer (the middle of our allocation), so
*any* `free(buf)` it attempts is an invalid free that aborts the process. The
call runs in a child process: child killed by signal (e.g. SIGABRT) = FAIL
"freed the buffer"; child exits 0 after freeing our allocation cleanly = PASS.
No cooperation from the C code is needed.

### 3c. Hold-and-use-late — poison + forced reuse

Immediately after C returns, the whole allocation is poisoned (`0xFE`), then a
second access is forced (`c_stash_reuse()`). If C saved the pointer, the reuse
writes the marker `0xEE` into the poison = FAIL. Correct code stashes nothing:
reuse is a no-op, poison intact = PASS. Stash globals are reset between runs.

### 3d. Read-past-length (read-only) — page guard

Pure reads leave no trace in canaries (the suite *proves* this with a test
asserting guards PASS on an over-read). So the buffer is placed to end exactly
at a protected `PROT_NONE` page (`mmap`/`mprotect`): reading `buf[len]` faults
the child = FAIL caught. In-bounds-only readers survive = PASS.

**Layouts:** contiguous guards for all fuzz sizes; page-guard dual runs for
exactly the 7 locked sizes `0, 1, 2, 16, 17, 4095, 4096` (overrun/underrun);
page-guard verdicts for every over-read input (the only layout that sees reads).

## 4. Quickstart

Prerequisites: Rust toolchain (`cargo`, `rustc`) and a C compiler (`cc`) — no
other dependencies (no crates beyond std).

```sh
git clone https://github.com/bm-a/raptor.git
cd raptor
cargo test            # full suite — expect 14 passed, 0 failed
cargo build           # builds ./target/debug/raptor
```

Single check (buggy overrun — expect FAIL, exit 1):

```sh
R='extern "C" fn f(buf: *mut u8, len: usize)'
C='void f(uint8_t *buf, size_t len)'
./target/debug/raptor check --rust-sig "$R" --c-sig "$C" --func overrun --len 16 --seed 7
```

Fuzz one function with 100 varied inputs (expect PASS = harness correct):

```sh
./target/debug/raptor check --rust-sig "$R" --c-sig "$C" --func overrun --fuzz 100
```

## 5. CLI reference

```
raptor check --rust-sig '<rust decl>' --c-sig '<c decl>' --func FUNC [--len N --seed S | --fuzz N]
```

| Flag | Meaning |
|---|---|
| `--rust-sig`, `--c-sig` | Declarations; both must match the flat-buffer pattern (pointer + length), else exit 2 |
| `--func` | `good \| overrun \| underrun \| free \| overread \| stash` (`c_` aliases accepted) |
| `--len N --seed S` | Single mode (defaults 16, 7): one guarded call + full report |
| `--fuzz N` | Fuzz mode: N cases (7 locked sizes + random lengths, all relevant layouts) + summary |

Exit codes: `0` = SAFE (single good) or harness-correct summary (fuzz);
`1` = UNSAFE / bug caught; `2` = usage or signature-pattern error.
Hidden children `__page-probe` / `__free-probe` isolate faults and aborts so
one buggy call can never crash the runner.

## 6. Understanding the report

Every run prints: function tested, input (length, seed, hex content preview),
the contract, one `[PASS]`/`[FAIL]` line per check, and a verdict. A FAIL
always includes a `Reproduce:` command, e.g.:

```
raptor report
Function tested: c_overrun
Input: length=16 seed=7 content=[c7 c4 1f e3 ... (16 bytes total)]
Contract: C may touch buf[0..len] only, during this call only.
[PASS] Guard placement has zero padding: OK — guards sit immediately against the buffer
[PASS] Write-before-start (leading guard intact?): yes, untouched
[FAIL] Write-past-end (trailing guard intact?): NO — trailing guard byte 0 was changed
Result: UNSAFE — caught on this input (reproduce: raptor check --func c_overrun --len 16 --seed 7).
```

Reference single-run outcomes (verified on the build machine):

| func | exit | verdict |
|---|---|---|
| good | 0 | SAFE on this input |
| overrun / underrun | 1 | UNSAFE — caught |
| free | 1 | UNSAFE — freed the buffer |
| overread | 1 | UNSAFE — over-read caught |
| stash | 1 | UNSAFE — late use caught |

## 7. Test suite & proof

The suite deliberately ships **buggy C functions** (`c_src/buggy.c`) so the
tool must prove it catches failures instead of always reporting "safe":
`c_good` (correct) vs `c_overrun`, `c_underrun`, `c_free_it`, `c_overread`,
`c_stash`+`c_stash_reuse`. Each bug type is fuzzed **100 runs** over varied
sizes/content (including edge sizes `0, 1, 2, 16, 17, 4095, 4096`) with
deterministic xorshift content reproducible from `(len, seed)`.

Success bar (met — see [How to verify](https://github.com/bm-a/raptor/wiki/How-to-verify)):

```
overrun: 100 runs, 0 misses · underrun: 100 runs, 0 misses
free-it: 100 runs, 0 misses · stash: 100 runs, 0 misses
overread/page: 100 runs, 0 misses · good: 100 runs, 0 false positives
```

Tests: `tests/step1_guard.rs` (6), `tests/step2_free_stash.rs` (5),
`tests/step3_overread.rs` (3). Every claim in this README has a command in
[the verification guide](https://github.com/bm-a/raptor/wiki/How-to-verify) —
if a command doesn't reproduce, that's a bug; file it with the FAIL block.

## 8. Project structure

```
raptor/
├── src/
│   ├── lib.rs        # all four checks: guards, free probe, stash/poison, page guard
│   └── main.rs       # `raptor check` CLI (single + fuzz), hidden child probes
├── c_src/
│   ├── buggy.h/.c    # 1 correct + 5 deliberately-buggy C functions (the proof)
├── tests/
│   ├── step1_guard.rs       # overrun/underrun, 100-run fuzz, page dual-layout
│   ├── step2_free_stash.rs  # free + stash, 100-run fuzz each
│   └── step3_overread.rs    # over-read via page guard + honest contiguous gap demo
├── build.rs          # compiles c_src/buggy.c (no external crates, works offline)
├── wiki/             # canonical wiki sources (mirrored to the GitHub wiki)
├── llms.txt          # machine-readable summary for AI agents
├── DESIGN.md         # locked design + amendments
└── DECISIONS.md      # plain-language trade-off log (#1–#10)
```

## 9. How raptor compares to existing tools

| Tool | What it does | What it misses (why raptor adds something) |
|---|---|---|
| `bindgen` / `cbindgen` | Generate FFI glue (declarations) | Zero checking of what C *does* with pointers |
| `cxx` crate | Type-level safety for Rust↔C++ | Trusts function bodies; C++-only |
| Miri | UB detection inside Rust | Blind inside real C ("stops tracking this memory") |
| AddressSanitizer / Valgrind | General C memory-error detection | Don't know your contract; no per-contract report |

raptor starts from the contract (this pointer, this length, read-only or not,
valid until return), harnesses it automatically, and reports per-check
PASS/FAIL with the failing input — using sanitizer-style techniques (canaries,
poisoning, guard pages, child isolation) under the hood. It complements these
tools; it replaces none.

## 10. Limitations (honest list)

- Corruption of unrelated memory far from the buffer.
- Pure over-reads on *writable* buffers (indistinguishable from legal reads).
- Nested pointers (flat outer buffer only), threading/races, leaks, unfuzzed inputs, C++.
- Late-use the tests never trigger (dynamic testing can't prove absence of an untriggered bug).
- Test/debug builds only — no production protection.
- One pattern, Rust-and-C only. **raptor is a test harness, not a new
  language, and v1 will not become one.**

## 11. Scope & roadmap

v1 is complete and locked: the four checks above for the single flat-buffer
pattern. Proposals (need explicit approval, none started): pointing the
harness at arbitrary user C functions, additional buffer patterns, additional
language pairs. If you believe the tool should be extended, open an issue in
plain language.

## 12. For AI agents

Machine-readable summary: [`llms.txt`](llms.txt). Key facts: zero-dependency
Rust binary + C test corpus; `cargo test` = 14 tests; CLI `raptor check
--func <name> [--len N --seed S | --fuzz N]`; exit 0 = safe/correct summary,
1 = bug caught, 2 = usage error; reports are self-contained (input +
reproduce command included); do not claim safety beyond the four checked
properties — cite §10.

## 13. Contributing

Bug reports: paste the full FAIL block (function, length, seed, content,
reproduce command) — everything needed is printed. Code changes: keep the
verifiability contract (every new check ships with a deliberately-buggy
function that proves it fires, plus fuzz runs). Keep `DESIGN.md`/`DECISIONS.md`
updated in plain language. No new FFI patterns or languages without prior
approval (see §11).

## 14. License

Licensed under either of Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
or MIT license ([LICENSE-MIT](LICENSE-MIT)) at your option — the standard
Rust-ecosystem dual license. Unless you explicitly state otherwise, any
contribution intentionally submitted for inclusion in this project shall be
dual-licensed as above, without additional terms.
