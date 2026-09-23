// raptor CLI: `raptor check` runs guarded checks and prints a plain-language
// report. Hidden children (__page-probe, __free-probe) isolate faults/aborts.
use raptor::{
    free_probe_main, interpret_free_status, make_content, page_probe_main, placement_ok, preview,
    run_guarded, run_stash_check, sigs_match_flat_buffer_pattern, TestFunc, PAGE_SIZES,
};
use std::env;
use std::process::{Command, ExitCode};

fn usage() -> String {
    "\
raptor check --rust-sig '<rust decl>' --c-sig '<c decl>' --func FUNC [--len N --seed S | --fuzz N]
  FUNC = good|overrun|underrun|free|overread|stash  (c_ aliases accepted)
  single: --len N --seed S (defaults 16, 7)
  fuzz:   --fuzz N  (N cases: 7 locked sizes + random lens, all relevant layouts)
  e.g. raptor check --rust-sig 'extern \"C\" fn f(buf: *mut u8, len: usize)' --c-sig 'void f(uint8_t *buf, size_t len)' --func overrun --fuzz 100
"
    .to_string()
}

fn get(args: &[String], flag: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
}

fn child_status(
    exe: &str,
    mode: &str,
    func: &str,
    len: usize,
    seed: u64,
) -> std::process::ExitStatus {
    Command::new(exe)
        .args([
            mode,
            "--func",
            func,
            "--len",
            &len.to_string(),
            "--seed",
            &seed.to_string(),
        ])
        .status()
        .expect("spawn raptor child probe")
}

fn fuzz_lens(n: usize, seed: u64) -> Vec<usize> {
    let mut lens: Vec<usize> = PAGE_SIZES.to_vec();
    let mut x = seed | 1;
    while lens.len() < n {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        lens.push((x % 8192) as usize);
    }
    lens.truncate(n);
    lens
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    // ---- hidden children ----
    if args.len() >= 2 && args[1] == "__page-probe" {
        let func = get(&args, "--func").unwrap_or_default();
        let len: usize = get(&args, "--len").unwrap_or_default().parse().unwrap_or(0);
        let seed: u64 = get(&args, "--seed")
            .unwrap_or_default()
            .parse()
            .unwrap_or(0);
        let f = TestFunc::from_name(&func).unwrap_or(TestFunc::Good);
        #[cfg(unix)]
        {
            let code = unsafe { page_probe_main(f, len, seed) };
            return ExitCode::from(code as u8);
        }
        #[cfg(not(unix))]
        {
            let _ = (f, len, seed);
            eprintln!("page layout only supported on unix");
            return ExitCode::from(99);
        }
    }
    if args.len() >= 2 && args[1] == "__free-probe" {
        let func = get(&args, "--func").unwrap_or_default();
        let len: usize = get(&args, "--len").unwrap_or_default().parse().unwrap_or(0);
        let seed: u64 = get(&args, "--seed")
            .unwrap_or_default()
            .parse()
            .unwrap_or(0);
        let f = TestFunc::from_name(&func).unwrap_or(TestFunc::Good);
        #[cfg(unix)]
        {
            let code = unsafe { free_probe_main(f, len, seed) };
            return ExitCode::from(code as u8);
        }
        #[cfg(not(unix))]
        {
            let _ = (f, len, seed);
            eprintln!("free probe only supported on unix");
            return ExitCode::from(99);
        }
    }
    if args.len() < 2 || args[1] != "check" {
        eprint!("{}", usage());
        return ExitCode::from(2);
    }
    let rust_sig = get(&args, "--rust-sig").unwrap_or_default();
    let c_sig = get(&args, "--c-sig").unwrap_or_default();
    let func_s = get(&args, "--func").unwrap_or_default();
    if let Err(e) = sigs_match_flat_buffer_pattern(&rust_sig, &c_sig) {
        eprintln!("raptor: signature problem\n  {}", e);
        eprintln!(
            "Only one pattern is supported: flat buffer (pointer + length), Rust-and-C only."
        );
        return ExitCode::from(2);
    }
    let Some(func) = TestFunc::from_name(&func_s) else {
        eprintln!("raptor: unknown --func '{}'\n{}", func_s, usage());
        return ExitCode::from(2);
    };

    if let Some(nstr) = get(&args, "--fuzz") {
        let n: usize = nstr.parse().unwrap_or(100);
        return fuzz_mode(func, n);
    }
    let len: usize = get(&args, "--len")
        .unwrap_or_else(|| "16".into())
        .parse()
        .unwrap_or(16);
    let seed: u64 = get(&args, "--seed")
        .unwrap_or_else(|| "7".into())
        .parse()
        .unwrap_or(7);
    single_mode(func, len, seed)
}

fn header(func: TestFunc, len: usize, seed: u64, content: &[u8]) -> String {
    format!(
        "raptor report\nFunction tested: {}\nInput: length={} seed={} content=[{}]\nContract: C may touch buf[0..len] only, during this call only.\n",
        func.c_name(),
        len,
        seed,
        preview(content)
    )
}

fn single_mode(func: TestFunc, len: usize, seed: u64) -> ExitCode {
    let exe = env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut out = String::new();
    let mut bad = false;

    match func {
        TestFunc::Good | TestFunc::Overrun | TestFunc::Underrun => {
            let (r, content) = run_guarded(func, len, seed);
            out.push_str(&header(func, len, seed, &content));
            out.push_str(&r.report(&content));
            if PAGE_SIZES.contains(&len) {
                let st = child_status(&exe, "__page-probe", func.c_name(), len, seed);
                out.push_str(&format!(
                    "[{}] Page-guard layout (size {:?}): {}\n",
                    if st.success() && st.code() == Some(0) {
                        "PASS"
                    } else {
                        "FAIL"
                    },
                    PAGE_SIZES,
                    if st.success() && st.code() == Some(0) {
                        "survived, leading guard intact".to_string()
                    } else {
                        "caught (fault or guard mismatch)".to_string()
                    }
                ));
                // For good, page must pass; for buggy, page must catch.
                let page_should_pass = func == TestFunc::Good;
                let page_passed = st.success() && st.code() == Some(0);
                if page_should_pass != page_passed {
                    // mismatch means verdict differs from expectation, but the
                    // report line above already shows it; mark overall below.
                }
                bad = !r.passed() || (page_should_pass != page_passed);
                // For buggy funcs, overall = caught (expected FAIL somewhere).
                if func != TestFunc::Good {
                    bad = r.passed() && page_passed; // true only if BOTH missed
                    if bad {
                        out.push_str(
                            "Result: MISSED BUG — neither layout caught it on this input.\n",
                        );
                    } else {
                        out.push_str(&format!("Result: UNSAFE — caught on this input (reproduce: raptor check --func {} --len {} --seed {}).\n", func.c_name(), len, seed));
                    }
                    print!("{}", out);
                    return if bad {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::from(1)
                    };
                }
                out.push_str(if bad {
                    "Result: UNSAFE on this input.\n"
                } else {
                    "Result: SAFE on this input.\n"
                });
            } else {
                bad = !r.passed();
                if bad {
                    out.push_str(&format!("Result: UNSAFE — C wrote outside the buffer (reproduce: raptor check --func {} --len {} --seed {}).\n", func.c_name(), len, seed));
                } else {
                    out.push_str("Result: SAFE on this input — no out-of-bounds write seen.\n");
                }
            }
        }
        TestFunc::FreeIt => {
            let content = make_content(len, seed);
            out.push_str(&header(func, len, seed, &content));
            let st = child_status(&exe, "__free-probe", "c_free_it", len, seed);
            let v = interpret_free_status(st);
            out.push_str(&v.report());
            out.push('\n');
            bad = v != raptor::FreeVerdict::NoFree;
            // For the deliberately-buggy freezer, "bad" (caught) is expected.
            if bad {
                out.push_str(&format!("Result: UNSAFE — freed the buffer (reproduce: raptor check --func c_free_it --len {} --seed {}).\n", len, seed));
            } else {
                out.push_str("Result: MISSED BUG — freezer was NOT caught on this input.\n");
            }
            print!("{}", out);
            return if bad {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            };
        }
        TestFunc::Overread => {
            let (r, content) = run_guarded(func, len, seed);
            out.push_str(&header(func, len, seed, &content));
            out.push_str(
                "Note: plain guard bytes cannot catch pure reads (reads leave no trace).\n",
            );
            out.push_str(&r.report(&content));
            // Over-read verdict always comes from the page layout (works for
            // any size; the locked-7 list governs overrun/underrun dual runs).
            assert!(placement_ok(len));
            let st = child_status(&exe, "__page-probe", "c_overread", len, seed);
            let caught = !(st.success() && st.code() == Some(0));
            out.push_str(&format!(
                "[{}] Page-guard over-read check: {}\n",
                if caught { "FAIL (bug caught)" } else { "PASS" },
                if caught {
                    "child faulted on buf[len] — over-read caught".to_string()
                } else {
                    "child survived — over-read NOT caught here".to_string()
                }
            ));
            if caught {
                out.push_str(&format!("Result: UNSAFE — over-read caught (reproduce: raptor check --func c_overread --len {} --seed {}).\n", len, seed));
            } else {
                out.push_str("Result: MISSED BUG — over-read not caught on this input.\n");
            }
            print!("{}", out);
            return if caught {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            };
        }
        TestFunc::Stash => {
            let (r, content) = run_stash_check(func, len, seed);
            out.push_str(&header(func, len, seed, &content));
            out.push_str(&r.report());
            bad = r.caught();
            if bad {
                out.push_str(&format!("Result: UNSAFE — late use caught (reproduce: raptor check --func c_stash --len {} --seed {}).\n", len, seed));
            } else {
                out.push_str("Result: MISSED BUG — stash was NOT caught on this input.\n");
            }
            print!("{}", out);
            return if bad {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            };
        }
    }
    print!("{}", out);
    if bad {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn fuzz_mode(func: TestFunc, n: usize) -> ExitCode {
    let exe = env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    // Distinct xorshift streams per func so inputs vary.
    let fseed = match func {
        TestFunc::Good => 0x1234,
        TestFunc::Overrun => 0xABCD,
        TestFunc::Underrun => 0xBEEF,
        TestFunc::FreeIt => 0xF4EE,
        TestFunc::Overread => 0x0EAD,
        TestFunc::Stash => 0x57A54,
    };
    let lens = fuzz_lens(n, fseed);
    let mut misses = 0usize;
    let mut false_alarms = 0usize;
    let mut first_fail: Option<String> = None;
    for (i, &len) in lens.iter().enumerate() {
        let seed = fseed ^ (1000 + i as u64);
        match func {
            TestFunc::Good => {
                let (r, content) = run_guarded(func, len, seed);
                if !r.passed() {
                    false_alarms += 1;
                    if first_fail.is_none() {
                        first_fail = Some(format!(
                            "len={} seed={} content=[{}]",
                            len,
                            seed,
                            preview(&content)
                        ));
                    }
                }
                // page + free + stash must also pass for good:
                if PAGE_SIZES.contains(&len) {
                    let st = child_status(&exe, "__page-probe", "c_good", len, seed);
                    if !(st.success() && st.code() == Some(0)) {
                        false_alarms += 1;
                        if first_fail.is_none() {
                            first_fail = Some(format!("page len={} seed={}", len, seed));
                        }
                    }
                }
                let fst = child_status(&exe, "__free-probe", "good", len, seed);
                if interpret_free_status(fst) != raptor::FreeVerdict::NoFree {
                    false_alarms += 1;
                    if first_fail.is_none() {
                        first_fail = Some(format!("free len={} seed={}", len, seed));
                    }
                }
                let (sr, _) = run_stash_check(TestFunc::Good, len, seed);
                if sr.caught() {
                    false_alarms += 1;
                    if first_fail.is_none() {
                        first_fail = Some(format!("stash len={} seed={}", len, seed));
                    }
                }
                // overread-good = plain read inside bounds via page probe (c_good)
            }
            TestFunc::Overrun | TestFunc::Underrun => {
                let (r, _) = run_guarded(func, len, seed);
                let mut caught = !r.passed();
                if PAGE_SIZES.contains(&len) {
                    let st = child_status(&exe, "__page-probe", func.c_name(), len, seed);
                    if !(st.success() && st.code() == Some(0)) {
                        caught = true;
                    }
                }
                if !caught {
                    misses += 1;
                    if first_fail.is_none() {
                        first_fail = Some(format!("len={} seed={}", len, seed));
                    }
                }
            }
            TestFunc::FreeIt => {
                let st = child_status(&exe, "__free-probe", "c_free_it", len, seed);
                if interpret_free_status(st) == raptor::FreeVerdict::NoFree {
                    misses += 1;
                    if first_fail.is_none() {
                        first_fail = Some(format!("len={} seed={}", len, seed));
                    }
                }
            }
            TestFunc::Overread => {
                // Contiguous layout is EXPECTED to miss (honest gap); verdict
                // comes from the page layout for locked sizes, and for other
                // sizes we count contiguous as not-applicable. To meet "zero
                // misses across 100 runs", fuzz for overread runs the page
                // probe for the 7 locked sizes; non-locked sizes are checked
                // by running the same input THROUGH the page layout too
                // (page layout works for any size — the lock is about what we
                // promise, not what the code can do). Every input must fault.
                let st = child_status(&exe, "__page-probe", "c_overread", len, seed);
                if st.success() && st.code() == Some(0) {
                    misses += 1;
                    if first_fail.is_none() {
                        first_fail = Some(format!("len={} seed={}", len, seed));
                    }
                }
            }
            TestFunc::Stash => {
                let (sr, _) = run_stash_check(func, len, seed);
                if !sr.caught() {
                    misses += 1;
                    if first_fail.is_none() {
                        first_fail = Some(format!("len={} seed={}", len, seed));
                    }
                }
            }
        }
    }
    println!("raptor fuzz report");
    println!("Function: {}  runs: {}", func.c_name(), lens.len());
    if func == TestFunc::Good {
        println!("false positives: {}", false_alarms);
        if let Some(f) = first_fail {
            println!("first failing input: {}", f);
        }
        println!(
            "{}",
            if false_alarms == 0 {
                "Result: PASS — zero false positives."
            } else {
                "Result: FAIL — false positives seen."
            }
        );
        return if false_alarms == 0 {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        };
    }
    println!("misses (bug NOT caught): {}", misses);
    if let Some(f) = first_fail {
        println!("first missed input: {}", f);
    }
    println!(
        "{}",
        if misses == 0 {
            "Result: PASS — bug caught on every run."
        } else {
            "Result: FAIL — bug missed."
        }
    );
    if misses == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
