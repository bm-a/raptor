# How to verify (no code reading needed)

Every claim below has a command you can run and output you can compare. If any
command fails to reproduce, that is a bug — report it instead of trusting the
docs.

## 1. Full automated suite — expect 14 passed, 0 failed

```sh
cd ~/raptor
cargo test
```

Expected lines (among the test output):

```
overrun: 100 runs, 0 misses
underrun: 100 runs, 0 misses
free-it: 100 runs, 0 misses
stash: 100 runs, 0 misses
overread/page: 100 runs, 0 misses
good: 100 runs, 0 false positives
page layout: good passes, overrun caught, for all 7 locked sizes
```

To see the plain-language lines, add `-- --nocapture`:

```sh
cargo test -- --nocapture
```

## 2. Fuzz each function through the CLI — expect 6 × PASS

```sh
R='extern "C" fn f(buf: *mut u8, len: usize)'
C='void f(uint8_t *buf, size_t len)'
for f in good overrun underrun free overread stash; do
  ./target/debug/raptor check --rust-sig "$R" --c-sig "$C" --func $f --fuzz 100
done
```

- `good` → `Result: PASS — zero false positives.`
- each buggy one → `Result: PASS — bug caught on every run.`

(Here the outer PASS means "the harness behaved correctly": caught every bug,
alarmed on nothing correct.)

## 3. Single cases — every FAIL names its input

```sh
./target/debug/raptor check --rust-sig "$R" --c-sig "$C" --func overrun --len 16 --seed 7
# → [FAIL] Write-past-end … trailing guard byte 0 was changed
# → Result: UNSAFE … (exit code 1)

./target/debug/raptor check --rust-sig "$R" --c-sig "$C" --func good --len 16 --seed 7
# → all [PASS] … Result: SAFE on this input (exit code 0)
```

Reference outputs recorded on the build machine:

| func | exit | last line |
|---|---|---|
| good | 0 | `Result: SAFE on this input.` |
| overrun | 1 | `Result: UNSAFE — caught on this input (reproduce: raptor check --func c_overrun --len 16 --seed 7).` |
| underrun | 1 | `Result: UNSAFE — caught … --func c_underrun --len 16 --seed 7` |
| free | 1 | `Result: UNSAFE — freed the buffer (reproduce: raptor check --func c_free_it --len 16 --seed 7).` |
| overread | 1 | `Result: UNSAFE — over-read caught (reproduce: raptor check --func c_overread --len 16 --seed 7).` |
| stash | 1 | `Result: UNSAFE — late use caught (reproduce: raptor check --func c_stash --len 16 --seed 7).` |

A FAIL block always contains **length + seed + content preview + reproduce
command** — hand it to anyone; no source reading needed.
