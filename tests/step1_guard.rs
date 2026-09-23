//! Step 1 tests: guard-byte overrun/underrun check.
//! Each test prints plain-language lines; run with --nocapture to see them.
use raptor::{placement_ok, run_guarded, sigs_match_flat_buffer_pattern, TestFunc, PAGE_SIZES};

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

#[test]
fn good_passes_100_runs_no_false_positives() {
    let lens = fuzz_lens(100, 0x1234);
    let mut fails = 0;
    for (i, &len) in lens.iter().enumerate() {
        let seed = 1000 + i as u64;
        let (r, content) = run_guarded(TestFunc::Good, len, seed);
        assert!(r.padding_ok, "padding must hold (len={})", len);
        if !r.passed() {
            fails += 1;
            println!(
                "UNEXPECTED FAIL len={} seed={}\n{}",
                len,
                seed,
                r.report(&content)
            );
        }
    }
    println!("good: 100 runs, {} false positives", fails);
    assert_eq!(
        fails, 0,
        "correct C function must pass with zero false positives"
    );
}

#[test]
fn overrun_caught_100_runs_no_misses() {
    let lens = fuzz_lens(100, 0xABCD);
    let mut missed = 0;
    for (i, &len) in lens.iter().enumerate() {
        let seed = 2000 + i as u64;
        let (r, content) = run_guarded(TestFunc::Overrun, len, seed);
        assert!(r.padding_ok, "padding must hold (len={})", len);
        if r.passed() {
            missed += 1;
            println!(
                "MISSED BUG len={} seed={}\n{}",
                len,
                seed,
                r.report(&content)
            );
        } else {
            assert!(
                !r.trailing_ok,
                "overrun must hit trailing guard (len={})",
                len
            );
            assert!(
                r.leading_ok,
                "overrun must not touch leading guard (len={})",
                len
            );
            assert_eq!(
                r.first_diff_trailing,
                Some(0),
                "1-byte overrun hits trailing byte 0"
            );
        }
    }
    println!("overrun: 100 runs, {} misses", missed);
    assert_eq!(missed, 0, "write-past-end must be caught every time");
}

#[test]
fn underrun_caught_100_runs_no_misses() {
    let lens = fuzz_lens(100, 0xBEEF);
    let mut missed = 0;
    for (i, &len) in lens.iter().enumerate() {
        let seed = 3000 + i as u64;
        let (r, content) = run_guarded(TestFunc::Underrun, len, seed);
        assert!(r.padding_ok, "padding must hold (len={})", len);
        if r.passed() {
            missed += 1;
            println!(
                "MISSED BUG len={} seed={}\n{}",
                len,
                seed,
                r.report(&content)
            );
        } else {
            assert!(
                !r.leading_ok,
                "underrun must hit leading guard (len={})",
                len
            );
            assert!(
                r.trailing_ok,
                "underrun must not touch trailing guard (len={})",
                len
            );
        }
    }
    println!("underrun: 100 runs, {} misses", missed);
    assert_eq!(missed, 0, "write-before-start must be caught every time");
}

#[test]
fn page_layout_covers_exactly_the_7_locked_sizes() {
    assert_eq!(PAGE_SIZES, [0, 1, 2, 16, 17, 4095, 4096]);
    for &len in &PAGE_SIZES {
        assert!(
            placement_ok(len),
            "page placement must hold for locked size {}",
            len
        );
    }
    println!("page placement OK for exactly {:?}", PAGE_SIZES);
}

#[test]
fn page_layout_good_passes_and_overrun_faults() {
    let exe = env!("CARGO_BIN_EXE_raptor");
    for &len in &PAGE_SIZES {
        let good = std::process::Command::new(exe)
            .args([
                "__page-probe",
                "--func",
                "good",
                "--len",
                &len.to_string(),
                "--seed",
                "7",
            ])
            .status()
            .expect("spawn page probe");
        assert!(
            good.success(),
            "good must survive page layout (len={})",
            len
        );

        let bad = std::process::Command::new(exe)
            .args([
                "__page-probe",
                "--func",
                "overrun",
                "--len",
                &len.to_string(),
                "--seed",
                "7",
            ])
            .status()
            .expect("spawn page probe");
        // Caught either as guard-mismatch exit(1) or as a fault (signal => !success).
        assert!(
            !bad.success(),
            "overrun must be caught by page guard (len={})",
            len
        );
    }
    println!("page layout: good passes, overrun caught, for all 7 locked sizes");
}

#[test]
fn sig_checker_accepts_flat_buffer_and_rejects_other() {
    assert!(sigs_match_flat_buffer_pattern(
        "extern \"C\" fn f(buf: *mut u8, len: usize)",
        "void f(uint8_t *buf, size_t len)"
    )
    .is_ok());
    assert!(sigs_match_flat_buffer_pattern("fn f(x: i32)", "void f(int x)").is_err());
}
