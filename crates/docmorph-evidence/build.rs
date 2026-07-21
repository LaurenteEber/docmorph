mod build_support;

use std::{env, process::Command};

fn main() {
    println!("cargo:rerun-if-env-changed=RUSTC");
    let rustc = env::var("RUSTC").expect("Cargo must provide RUSTC to build scripts");
    let output = Command::new(rustc)
        .arg("-Vv")
        .output()
        .expect("Cargo RUSTC -Vv must spawn");
    let compiler = build_support::compiler_identity_from_output(output)
        .expect("Cargo RUSTC -Vv must provide complete UTF-8 compiler identity");
    println!(
        "cargo:rustc-env=DOCMORPH_BUILD_RUSTC_RELEASE={}",
        compiler.release
    );
    println!(
        "cargo:rustc-env=DOCMORPH_BUILD_RUSTC_COMMIT={}",
        compiler.commit_hash
    );
    println!(
        "cargo:rustc-env=DOCMORPH_BUILD_RUSTC_HOST={}",
        compiler.host
    );
    println!(
        "cargo:rustc-env=DOCMORPH_BUILD_RUSTC_LLVM={}",
        compiler.llvm_version
    );
}
