//! raptor: runtime buffer-safety checks at the Rust-to-C FFI boundary.
//! One pattern only: Rust hands a flat buffer (pointer + length) to C for one call.
//! Checks: overrun/underrun (guards), free (child + abort detection),
//! hold-and-use-late (poison + forced reuse), over-read (page guard).
//! Plain-language docs live in README.md and DECISIONS.md.

pub const GUARD_SIZE: usize = 16;
pub const GUARD_BEFORE: u8 = 0xAA;
pub const GUARD_AFTER: u8 = 0xBB;
pub const POISON: u8 = 0xFE;
pub const STASH_WRITE: u8 = 0xEE;

/// Exact sizes that get BOTH layouts (contiguous + page-boundary). Closed list.
pub const PAGE_SIZES: [usize; 7] = [0, 1, 2, 16, 17, 4095, 4096];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestFunc {
    Good,
    Overrun,
    Underrun,
    FreeIt,
    Overread,
    Stash,
}

impl TestFunc {
    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "good" | "c_good" => Some(Self::Good),
            "overrun" | "c_overrun" => Some(Self::Overrun),
            "underrun" | "c_underrun" => Some(Self::Underrun),
            "free" | "free_it" | "c_free_it" => Some(Self::FreeIt),
            "overread" | "c_overread" => Some(Self::Overread),
            "stash" | "c_stash" => Some(Self::Stash),
            _ => None,
        }
    }
    pub fn c_name(&self) -> &'static str {
        match self {
            Self::Good => "c_good",
            Self::Overrun => "c_overrun",
            Self::Underrun => "c_underrun",
            Self::FreeIt => "c_free_it",
            Self::Overread => "c_overread",
            Self::Stash => "c_stash",
        }
    }
    pub fn all() -> [Self; 6] {
        [
            Self::Good,
            Self::Overrun,
            Self::Underrun,
            Self::FreeIt,
            Self::Overread,
            Self::Stash,
        ]
    }
}

// ---- FFI declarations (compiled from c_src/buggy.c by build.rs) ----
unsafe extern "C" {
    fn c_good(buf: *mut u8, len: usize);
    fn c_overrun(buf: *mut u8, len: usize);
    fn c_underrun(buf: *mut u8, len: usize);
    fn c_free_it(buf: *mut u8, len: usize);
    fn c_overread(buf: *const u8, len: usize);
    fn c_stash(buf: *mut u8, len: usize);
    fn c_stash_reuse() -> u8;
    fn c_stash_reset();
}

/// Deterministic filler so failures are reproducible from (len, seed).
pub fn make_content(len: usize, seed: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(len);
    let mut x = seed | 1; // xorshift must not be 0
    for _ in 0..len {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        out.push((x & 0xFF) as u8);
    }
    out
}

/// Short hex preview of the input content (first 16 bytes max).
pub fn preview(content: &[u8]) -> String {
    let n = content.len().min(16);
    let hex: Vec<String> = content[..n].iter().map(|b| format!("{:02x}", b)).collect();
    if content.len() > 16 {
        format!("{}... ({} bytes total)", hex.join(" "), content.len())
    } else if content.is_empty() {
        "(empty buffer)".to_string()
    } else {
        format!("{} ({} bytes total)", hex.join(" "), content.len())
    }
}

// ---------------------------------------------------------------------------
// Step 1: guard-byte check (contiguous layout).
// NOTE: calling run_guarded(FreeIt) in-process would abort the runner
// (free of an interior pointer), so FreeIt is ONLY run in the __free-probe
// child. run_guarded deliberately panics for FreeIt with a clear message.
// Overread in-process intentionally PASSES guards (reads don't disturb them)
// to honestly demonstrate why the page layout (Step 3) is needed.
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct CheckResult {
    pub func: TestFunc,
    pub len: usize,
    pub seed: u64,
    pub padding_ok: bool,
    pub leading_ok: bool,
    pub trailing_ok: bool,
    pub first_diff_leading: Option<usize>,
    pub first_diff_trailing: Option<usize>,
}

impl CheckResult {
    pub fn passed(&self) -> bool {
        self.padding_ok && self.leading_ok && self.trailing_ok
    }
    pub fn report(&self, content: &[u8]) -> String {
        let mut s = String::new();
        s.push_str(&format!(
            "[{}] Guard placement has zero padding: {}\n",
            if self.padding_ok { "PASS" } else { "FAIL" },
            if self.padding_ok {
                "OK — guards sit immediately against the buffer".to_string()
            } else {
                "BROKEN HARNESS — padding detected, result untrustworthy".to_string()
            }
        ));
        s.push_str(&format!(
            "[{}] Write-before-start (leading guard intact?): {}\n",
            if self.leading_ok { "PASS" } else { "FAIL" },
            match self.first_diff_leading {
                None => "yes, untouched".to_string(),
                Some(i) => format!("NO — leading guard byte {} was changed", i),
            }
        ));
        s.push_str(&format!(
            "[{}] Write-past-end (trailing guard intact?): {}\n",
            if self.trailing_ok { "PASS" } else { "FAIL" },
            match self.first_diff_trailing {
                None => "yes, untouched".to_string(),
                Some(i) => format!("NO — trailing guard byte {} was changed", i),
            }
        ));
        s
    }
}

pub fn run_guarded(func: TestFunc, len: usize, seed: u64) -> (CheckResult, Vec<u8>) {
    if func == TestFunc::FreeIt {
        panic!("run_guarded(FreeIt) would abort this process (free of interior pointer): use the __free-probe child");
    }
    let content = make_content(len, seed);
    let total = GUARD_SIZE + len + GUARD_SIZE;
    let mut alloc = vec![0u8; total];
    alloc[..GUARD_SIZE].fill(GUARD_BEFORE);
    alloc[GUARD_SIZE..GUARD_SIZE + len].copy_from_slice(&content);
    alloc[GUARD_SIZE + len..].fill(GUARD_AFTER);

    let base = alloc.as_mut_ptr();
    let buf = unsafe { base.add(GUARD_SIZE) };
    let pad_ok =
        unsafe { base.add(GUARD_SIZE) == buf && buf.add(len) == base.add(GUARD_SIZE + len) };

    unsafe {
        match func {
            TestFunc::Good => c_good(buf, len),
            TestFunc::Overrun => c_overrun(buf, len),
            TestFunc::Underrun => c_underrun(buf, len),
            TestFunc::Overread => c_overread(buf, len),
            TestFunc::Stash => c_stash(buf, len),
            TestFunc::FreeIt => unreachable!(),
        }
    }

    let mut first_lead = None;
    for i in 0..GUARD_SIZE {
        if alloc[i] != GUARD_BEFORE && first_lead.is_none() {
            first_lead = Some(i);
        }
    }
    let mut first_trail = None;
    for i in 0..GUARD_SIZE {
        if alloc[GUARD_SIZE + len + i] != GUARD_AFTER && first_trail.is_none() {
            first_trail = Some(i);
        }
    }
    let r = CheckResult {
        func,
        len,
        seed,
        padding_ok: pad_ok,
        leading_ok: first_lead.is_none(),
        trailing_ok: first_trail.is_none(),
        first_diff_leading: first_lead,
        first_diff_trailing: first_trail,
    };
    (r, content)
}

// ---------------------------------------------------------------------------
// Step 2a: free check. The C function only ever receives an INTERIOR pointer
// (middle of our allocation), so ANY free(buf) it attempts is an invalid free
// and aborts the process. We run the call in a child (__free-probe): a dead
// child means "C freed (or heap-corrupted) the buffer" = bug caught; a live
// child that exits 0 means no free happened.
// ---------------------------------------------------------------------------

#[cfg(unix)]
pub mod freeprobe {
    use super::TestFunc;

    unsafe extern "C" {
        fn malloc(size: usize) -> *mut u8;
        fn free(ptr: *mut u8);
    }

    /// Child body. Exit 0 = survived with guards intact (no free).
    /// Exit 1 = survived but guards disturbed. Abort/signal = freed (caught).
    /// Must run in a child process.
    pub unsafe fn probe(func: TestFunc, len: usize, seed: u64) -> i32 {
        unsafe extern "C" {
            fn c_good(buf: *mut u8, len: usize);
            fn c_free_it(buf: *mut u8, len: usize);
        }
        let total = super::GUARD_SIZE + len + super::GUARD_SIZE;
        let base = unsafe { malloc(total.max(1)) };
        if base.is_null() {
            return 99;
        }
        let buf = unsafe { base.add(super::GUARD_SIZE) };
        let content = super::make_content(len, seed);
        unsafe {
            core::ptr::write_bytes(base, super::GUARD_BEFORE, super::GUARD_SIZE);
            if len > 0 {
                core::ptr::copy_nonoverlapping(content.as_ptr(), buf, len);
            }
            core::ptr::write_bytes(
                base.add(super::GUARD_SIZE + len),
                super::GUARD_AFTER,
                super::GUARD_SIZE,
            );
        }
        unsafe {
            match func {
                TestFunc::Good => c_good(buf, len),
                TestFunc::FreeIt => c_free_it(buf, len),
                _ => {
                    free(base);
                    return 98;
                }
            }
        }
        // Survived: check guards, then free our own allocation.
        let mut ok = true;
        for i in 0..super::GUARD_SIZE {
            if unsafe { *base.add(i) } != super::GUARD_BEFORE {
                ok = false;
                break;
            }
            if unsafe { *base.add(super::GUARD_SIZE + len + i) } != super::GUARD_AFTER {
                ok = false;
                break;
            }
        }
        unsafe { free(base) };
        if ok {
            0
        } else {
            1
        }
    }
}

#[cfg(unix)]
pub unsafe fn free_probe_main(func: TestFunc, len: usize, seed: u64) -> i32 {
    unsafe { freeprobe::probe(func, len, seed) }
}

#[derive(Debug, PartialEq, Eq)]
pub enum FreeVerdict {
    /// Child exited 0: C did not free, guards intact.
    NoFree,
    /// Child exited 1: survived but wrote out of bounds.
    GuardMismatch,
    /// Child died (signal/abort) or bad exit: C freed or heap-corrupted.
    FreedOrCrashed { code_or_signal: String },
}

impl FreeVerdict {
    pub fn report(&self) -> String {
        match self {
            Self::NoFree => "[PASS] No-free (child freed our allocation cleanly)".to_string(),
            Self::GuardMismatch => {
                "[FAIL] No-free: child survived but guards disturbed".to_string()
            }
            Self::FreedOrCrashed { code_or_signal } => format!(
                "[FAIL] Freed the buffer (child died: {}) — C called free() on memory it does not own",
                code_or_signal
            ),
        }
    }
}

/// Parent-side helper: interpret a child ExitStatus for the free probe.
pub fn interpret_free_status(st: std::process::ExitStatus) -> FreeVerdict {
    if let Some(code) = st.code() {
        if code == 0 {
            FreeVerdict::NoFree
        } else if code == 1 {
            FreeVerdict::GuardMismatch
        } else {
            FreeVerdict::FreedOrCrashed {
                code_or_signal: format!("exit code {}", code),
            }
        }
    } else {
        FreeVerdict::FreedOrCrashed {
            code_or_signal: "killed by signal (e.g. SIGABRT from invalid free)".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Step 2b: hold-and-use-late (stash) check. Call C, then poison the whole
// allocation, then force a second access via c_stash_reuse(). If C saved the
// pointer, the reuse writes 0xEE into the poison = bug caught. A correct C
// function never stashes, so reuse is a no-op and poison stays intact.
// c_stash_reset() is called before each run for hygiene (C global state).
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct StashResult {
    pub len: usize,
    pub seed: u64,
    pub stashed_func: TestFunc,
    pub reuse_ret: u8,
    pub poison_disturbed: bool,
    pub disturbed_offset: Option<usize>,
}

impl StashResult {
    /// For the buggy stasher, disturbed must be true. For good, false.
    pub fn caught(&self) -> bool {
        self.poison_disturbed
    }
    pub fn report(&self) -> String {
        let mut s = String::new();
        s.push_str(&format!(
            "[{}] Hold-and-use-late (poison intact after forced reuse?): {}\n",
            if !self.poison_disturbed {
                "PASS"
            } else {
                "FAIL"
            },
            if !self.poison_disturbed {
                "yes — second access touched nothing (no stashed pointer)".to_string()
            } else {
                format!(
                    "NO — byte at offset {} changed after return (reuse_ret=0x{:02x})",
                    self.disturbed_offset.unwrap_or(usize::MAX),
                    self.reuse_ret
                )
            }
        ));
        s
    }
}

pub fn run_stash_check(func: TestFunc, len: usize, seed: u64) -> (StashResult, Vec<u8>) {
    let content = make_content(len, seed);
    let total = GUARD_SIZE + len + GUARD_SIZE;
    let mut alloc = vec![0u8; total];
    alloc[..GUARD_SIZE].fill(GUARD_BEFORE);
    alloc[GUARD_SIZE..GUARD_SIZE + len].copy_from_slice(&content);
    alloc[GUARD_SIZE + len..].fill(GUARD_AFTER);
    let base = alloc.as_mut_ptr();
    let buf = unsafe { base.add(GUARD_SIZE) };
    unsafe {
        c_stash_reset();
        match func {
            TestFunc::Stash => c_stash(buf, len),
            TestFunc::Good => c_good(buf, len),
            _ => c_stash(buf, len),
        }
        // Poison everything immediately after return.
        core::ptr::write_bytes(base, POISON, total);
        let ret = c_stash_reuse();
        // Scan for the stash write.
        let mut off = None;
        for i in 0..total {
            if alloc[i] != POISON {
                off = Some(i);
                break;
            }
        }
        let r = StashResult {
            len,
            seed,
            stashed_func: func,
            reuse_ret: ret,
            poison_disturbed: off.is_some(),
            disturbed_offset: off,
        };
        c_stash_reset();
        (r, content)
    }
}

// ---------------------------------------------------------------------------
// Page-boundary layout (PAGE_SIZES only). mmap + PROT_NONE guard page so an
// overrun OR over-read past the end faults. Child process (__page-probe).
// ---------------------------------------------------------------------------

#[cfg(unix)]
mod page {
    use super::TestFunc;

    const PROT_NONE: i32 = 0;
    const PROT_READ: i32 = 1;
    const PROT_WRITE: i32 = 2;
    const MAP_PRIVATE: i32 = 2;
    const MAP_ANONYMOUS: i32 = 32;

    unsafe extern "C" {
        fn mmap(addr: *mut u8, len: usize, prot: i32, flags: i32, fd: i32, offset: i64) -> *mut u8;
        fn mprotect(addr: *mut u8, len: usize, prot: i32) -> i32;
        fn munmap(addr: *mut u8, len: usize) -> i32;
        // getpagesize(), not sysconf: the sysconf constant differs per libc
        // (30 glibc vs 39 Bionic); wrong constant silently returned 1 here.
        fn getpagesize() -> i32;
    }

    pub fn page_size() -> usize {
        unsafe { getpagesize() as usize }
    }

    pub fn placement_ok(len: usize) -> bool {
        let ps = page_size();
        if ps == 0 {
            return false;
        }
        let data = super::GUARD_SIZE + len;
        let data_pages = data.div_ceil(ps);
        let _map = data_pages * ps + ps;
        data >= super::GUARD_SIZE + len
    }

    /// Exit 0 = guards intact, 1 = guard mismatch, fault = overrun/over-read
    /// into the PROT_NONE page (caught by parent as !success).
    pub unsafe fn probe(func: TestFunc, len: usize, seed: u64) -> i32 {
        unsafe extern "C" {
            fn c_good(buf: *mut u8, len: usize);
            fn c_overrun(buf: *mut u8, len: usize);
            fn c_underrun(buf: *mut u8, len: usize);
            fn c_overread(buf: *const u8, len: usize);
        }
        let ps = page_size();
        let data = super::GUARD_SIZE + len;
        let data_pages = data.div_ceil(ps).max(1);
        let map_len = data_pages * ps + ps;
        let map = unsafe {
            mmap(
                core::ptr::null_mut(),
                map_len,
                PROT_READ | PROT_WRITE,
                MAP_PRIVATE | MAP_ANONYMOUS,
                -1,
                0,
            )
        };
        if map as isize == -1 {
            return 99;
        }
        let guard_page = unsafe { map.add(data_pages * ps) };
        if unsafe { mprotect(guard_page, ps, PROT_NONE) } != 0 {
            unsafe { munmap(map, map_len) };
            return 99;
        }
        let data_end = unsafe { map.add(data_pages * ps) };
        let buf = unsafe { data_end.sub(len) };
        let lead = unsafe { buf.sub(super::GUARD_SIZE) };
        let content = super::make_content(len, seed);
        unsafe {
            core::ptr::write_bytes(lead, super::GUARD_BEFORE, super::GUARD_SIZE);
            if len > 0 {
                core::ptr::copy_nonoverlapping(content.as_ptr(), buf, len);
            }
        }
        if (lead as usize) < (map as usize) {
            unsafe { munmap(map, map_len) };
            return 99;
        }
        unsafe {
            match func {
                TestFunc::Good => c_good(buf, len),
                TestFunc::Overrun => c_overrun(buf, len),
                TestFunc::Underrun => c_underrun(buf, len),
                TestFunc::Overread => c_overread(buf as *const u8, len),
                // Free/stash are not page-probed (separate children/checks).
                _ => {
                    munmap(map, map_len);
                    return 98;
                }
            }
        }
        let mut ok = true;
        for i in 0..super::GUARD_SIZE {
            if unsafe { *lead.add(i) } != super::GUARD_BEFORE {
                ok = false;
                break;
            }
        }
        unsafe { munmap(map, map_len) };
        if ok {
            0
        } else {
            1
        }
    }
}

#[cfg(unix)]
pub use page::{page_size, placement_ok};

#[cfg(unix)]
pub unsafe fn page_probe_main(func: TestFunc, len: usize, seed: u64) -> i32 {
    unsafe { page::probe(func, len, seed) }
}

/// Minimal signature check: both sigs must describe the flat-buffer pattern.
pub fn sigs_match_flat_buffer_pattern(rust_sig: &str, c_sig: &str) -> Result<(), String> {
    for (which, s) in [("rust", rust_sig), ("c", c_sig)] {
        let has_ptr = s.contains('*') || s.contains("ptr") || s.contains("pointer");
        let has_len = s.contains("len")
            || s.contains("size_t")
            || s.contains("size")
            || s.contains("nbytes")
            || s.contains("length");
        if !(has_ptr && has_len) {
            return Err(format!(
                "{} signature does not look like the flat buffer pattern (need a pointer + a length in one call): '{}'",
                which, s
            ));
        }
    }
    Ok(())
}
