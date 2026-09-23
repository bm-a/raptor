// Build script: compile c_src/buggy.c with the system C compiler.
// No external crates used (works offline). Produces libbuggy.a in OUT_DIR.
use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let src = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("c_src/buggy.c");
    let obj = out.join("buggy.o");
    let lib = out.join("libbuggy.a");

    let cc = env::var("CC").unwrap_or_else(|_| "cc".to_string());

    let status = Command::new(&cc)
        .args([
            "-c",
            "-g",
            "-O0",
            "-Wall",
            "-Wextra",
            "-fno-omit-frame-pointer",
        ])
        .arg(&src)
        .arg("-o")
        .arg(&obj)
        .status()
        .expect("failed to run C compiler");
    assert!(status.success(), "compiling buggy.c failed");

    let status = Command::new("ar")
        .args(["crus"])
        .arg(&lib)
        .arg(&obj)
        .status()
        .expect("failed to run ar");
    assert!(status.success(), "archiving libbuggy.a failed");

    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=buggy");
    println!("cargo:rerun-if-changed=c_src/buggy.c");
    println!("cargo:rerun-if-changed=c_src/buggy.h");
}
