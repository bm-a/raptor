//! Step 3 tests: read-past-length on read-only buffers.
//! Honest two-part story: contiguous guards CANNOT catch pure reads (shown),
//! the page-guard layout catches them (shown), for all 100 fuzzed inputs.
use raptor::{run_guarded, TestFunc, PAGE_SIZES};

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

fn page_child(func: &str, len: usize, seed: u64) -> std::process::ExitStatus {
    let exe = env!("CARGO_BIN_EXE_raptor");
    std::process::Command::new(exe)
        .args([
            "__page-probe",
            "--func",
            func,
            "--len",
            &len.to_string(),
            "--seed",
            &seed.to_string(),
        ])
        .status()
        .expect("spawn page probe")
}

#[test]
fn contiguous_honestly_misses_pure_overread() {
    // Reads leave no trace in guard bytes: this MUST pass guards, proving why
    // Step 3 needs the page layout. If this ever fails, our story is wrong.
    let (r, _) = run_guarded(TestFunc::Overread, 16, 7);
    assert!(
        r.passed(),
        "contiguous guards cannot see pure reads — expected PASS here"
    );
    println!("confirmed gap: contiguous guards PASS on over-read (reads leave no trace)");
}

#[test]
fn overread_caught_by_page_guard_100_runs() {
    let lens = fuzz_lens(100, 0x0EAD);
    let mut missed = 0;
    for (i, &len) in lens.iter().enumerate() {
        let seed = 6000 + i as u64;
        let st = page_child("c_overread", len, seed);
        if st.success() && st.code() == Some(0) {
            missed += 1;
            println!("MISSED OVERREAD len={} seed={}", len, seed);
        }
    }
    println!("overread/page: 100 runs, {} misses", missed);
    assert_eq!(
        missed, 0,
        "read-past-length must fault on the guard page every time"
    );
}

#[test]
fn good_reads_inside_survive_page_guard() {
    // c_good only touches buf[0..len]; page layout must pass for locked sizes.
    for &len in &PAGE_SIZES {
        let st = page_child("good", len, 7);
        assert!(
            st.success() && st.code() == Some(0),
            "good must survive page layout (len={})",
            len
        );
    }
    println!("good survives page layout for all 7 locked sizes (no false over-read alarms)");
}
