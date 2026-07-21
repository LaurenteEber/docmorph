#[path = "../build_support.rs"]
mod build_support;

use std::process::Output;

use build_support::parse_rustc_verbose;

fn output(status: i32, stdout: &[u8], stderr: &[u8]) -> Output {
    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;

    Output {
        #[cfg(unix)]
        status: std::process::ExitStatus::from_raw(status << 8),
        stdout: stdout.into(),
        stderr: stderr.into(),
    }
}

#[test]
fn rejects_nonzero_compiler_status() {
    let error = build_support::compiler_identity_from_output(output(1, b"", b"compiler failed"))
        .expect_err("nonzero rustc exits are rejected");

    assert!(error.contains("status"));
}

#[test]
fn rejects_malformed_missing_and_duplicate_required_fields() {
    for source in [
        "release: 1.96.0\ncommit-hash: abc\nhost: test\n",
        "release: 1.96.0\ncommit-hash: abc\nhost: test\nLLVM version: 20\nrelease: again\n",
        "release: 1.96.0\ncommit-hash: abc\nhost test\nLLVM version: 20\n",
    ] {
        assert!(
            parse_rustc_verbose(source).is_err(),
            "invalid compiler output fails"
        );
    }
}

#[test]
fn parses_complete_compiler_identity() {
    let identity = parse_rustc_verbose(
        "rustc 1.96.0\nrelease: 1.96.0\ncommit-hash: abc123\nhost: aarch64-apple-darwin\nLLVM version: 20.1.8\n",
    )
    .expect("complete rustc verbose output parses");

    assert_eq!(identity.release, "1.96.0");
    assert_eq!(identity.commit_hash, "abc123");
    assert_eq!(identity.host, "aarch64-apple-darwin");
    assert_eq!(identity.llvm_version, "20.1.8");
}
