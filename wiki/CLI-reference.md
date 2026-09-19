# CLI reference

```
raptor check --rust-sig '<rust decl>' --c-sig '<c decl>' --func FUNC [--len N --seed S | --fuzz N]
```

- `FUNC` = `good | overrun | underrun | free | overread | stash`
  (`c_` aliases like `c_free_it` accepted).
- **Single mode** (default `--len 16 --seed 7`): one guarded call, full
  plain-language report. Exit `0` = SAFE (or, for `--func good`, pass);
  exit `1` = UNSAFE/caught; exit `2` = usage/signature error.
- **Fuzz mode** (`--fuzz N`): N cases per function — the 7 locked sizes plus
  random lengths (xorshift, distinct stream per function), all relevant
  layouts. Prints a summary; exit `0` = all caught (buggy) / zero false
  positives (good).
- **Signature check**: both `--rust-sig` and `--c-sig` must look like the flat
  buffer pattern (a pointer + a length in one call); anything else is rejected
  with exit 2. This is the tool refusing out-of-scope input, honestly.
- **Hidden children** (not for direct use): `raptor __page-probe ...` and
  `raptor __free-probe ...` isolate faults/aborts so one buggy call can't
  crash the runner. Exit `0` = survived intact, `1` = guard mismatch,
  killed-by-signal = fault/invalid-free (counts as caught).

## Examples

```sh
R='extern "C" fn f(buf: *mut u8, len: usize)'
C='void f(uint8_t *buf, size_t len)'
./target/debug/raptor check --rust-sig "$R" --c-sig "$C" --func stash --len 16 --seed 7
./target/debug/raptor check --rust-sig "$R" --c-sig "$C" --func free --fuzz 100
./target/debug/raptor check --rust-sig 'fn f(x: i32)' --c-sig 'void f(int x)' --func good
# → signature problem, exit 2 (not the flat-buffer pattern)
```
