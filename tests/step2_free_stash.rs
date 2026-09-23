//! Step 2 tests: free check + hold-and-use-late (stash) check.
//! Run with --nocapture to see the plain-language lines.
use raptor::{
    interpret_free_status, run_guarded, run_stash_check, FreeVerdict, TestFunc, PAGE_SIZES,
};

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

fn free_child(func: &str, len: usize, seed: u64) -> FreeVerdict {
    let exe = env!("CARGO_BIN_EXE_raptor");
    let st = std::process::Command::new(exe)
        .args([
            "__free-probe",
            "--func",
            func,
            "--len",
            &len.to_string(),
            "--seed",
            &seed.to_string(),
        ])
        .status()
        .expect("spawn free probe");
    interpret_free_status(st)
}

#[test]
fn free_bug_caught_100_runs_no_misses() {
    let lens = fuzz_lens(100, 0xF4EE);
    let mut missed = 0;
    for (i, &len) in lens.iter().enumerate() {
        let seed = 4000 + i as u64;
        let v = free_child("c_free_it", len, seed);
        if v == FreeVerdict::NoFree {
            missed += 1;
            println!("MISSED FREE len={} seed={}", len, seed);
        }
    }
    println!("free-it: 100 runs, {} misses", missed);
    assert_eq!(missed, 0, "free() of the buffer must be caught every time");
}

#[test]
fn good_never_flags_free_100_runs() {
    let lens = fuzz_lens(100, 0x1234);
    let mut false_alarms = 0;
    for (i, &len) in lens.iter().enumerate() {
        let seed = 4100 + i as u64;
        let v = free_child("good", len, seed);
        if v != FreeVerdict::NoFree {
            false_alarms += 1;
            println!("FALSE FREE-ALARM len={} seed={} verdict={:?}", len, seed, v);
        }
    }
    println!("good/free: 100 runs, {} false alarms", false_alarms);
    assert_eq!(false_alarms, 0, "correct function must never flag free");
}

#[test]
fn stash_bug_caught_100_runs_no_misses() {
    let lens = fuzz_lens(100, 0x57A54);
    let mut missed = 0;
    for (i, &len) in lens.iter().enumerate() {
        let seed = 5000 + i as u64;
        let (r, content) = run_stash_check(TestFunc::Stash, len, seed);
        if !r.caught() {
            missed += 1;
            println!(
                "MISSED STASH len={} seed={} content=[{}]",
                len,
                seed,
                raptor::preview(&content)
            );
        } else {
            // The reuse must write exactly our marker at buf[0].
            assert_eq!(r.reuse_ret, 0xEE, "reuse must show the stash marker");
        }
    }
    println!("stash: 100 runs, {} misses", missed);
    assert_eq!(missed, 0, "hold-and-use-late must be caught every time");
}

#[test]
fn good_never_flags_stash_100_runs() {
    let lens = fuzz_lens(100, 0x1234);
    let mut false_alarms = 0;
    for (i, &len) in lens.iter().enumerate() {
        let seed = 5100 + i as u64;
        let (r, _) = run_stash_check(TestFunc::Good, len, seed);
        // Good writes 0x42 into the buffer BEFORE poisoning; poison wipes it,
        // reuse is a no-op (nothing stashed), so poison must stay intact.
        if r.caught() {
            false_alarms += 1;
            println!("FALSE STASH-ALARM len={} seed={}", len, seed);
        }
    }
    println!("good/stash: 100 runs, {} false alarms", false_alarms);
    assert_eq!(false_alarms, 0, "correct function must never flag stash");
}

#[test]
fn stash_guards_still_checked_alongside() {
    // Stashing itself must not corrupt guards (the bug is the late use, not the call).
    let (r, _) = run_guarded(TestFunc::Stash, 16, 7);
    assert!(
        r.passed(),
        "c_stash call itself must leave guards intact; only the late reuse is the bug"
    );
    println!("stash call leaves guards intact; late reuse is what fails");
}
