use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use docmorph_core::io::{InputPolicy, validate_input};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after Unix epoch")
            .as_nanos();
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("docmorph-core-io-{unique}-{sequence}"));
        fs::create_dir(&path).expect("temporary root is created");
        Self { path }
    }

    fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.path.join(name);
        fs::write(&path, bytes).expect("input file is written");
        path
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn policy(root: &Path) -> InputPolicy {
    InputPolicy::new(vec![root.to_path_buf()])
}

#[test]
fn accepts_a_regular_file_within_an_allowed_canonical_root() {
    let root = TempRoot::new();
    let input = root.file("input.txt", b"safe local bytes");

    let validated = validate_input(&policy(&root.path), &input).expect("input is accepted");

    assert_eq!(validated.path(), input.canonicalize().unwrap());
    assert_eq!(validated.bytes(), b"safe local bytes");
}

#[test]
fn rejects_missing_and_disallowed_inputs_with_policy_diagnostics() {
    let root = TempRoot::new();
    let outside = TempRoot::new();
    let disallowed = outside.file("outside.txt", b"outside");

    let missing = validate_input(&policy(&root.path), root.path.join("missing.txt"));
    let outside_root = validate_input(&policy(&root.path), disallowed);

    assert_eq!(missing.unwrap_err().code, "input_missing");
    assert_eq!(outside_root.unwrap_err().code, "input_outside_allowed_root");
}

#[test]
fn rejects_files_larger_than_the_200_mb_limit_before_reading_them() {
    let root = TempRoot::new();
    let oversized = root.path.join("oversized.bin");
    let file = fs::File::create(&oversized).expect("oversized file is created");
    file.set_len(200 * 1024 * 1024 + 1)
        .expect("oversized length is set");

    let result = validate_input(&policy(&root.path), oversized);

    assert_eq!(result.unwrap_err().code, "input_too_large");
}

#[test]
fn rejects_directories_and_symlinks_that_resolve_outside_the_allowed_root() {
    let root = TempRoot::new();
    let outside = TempRoot::new();
    let escaped_target = outside.file("secret.txt", b"outside");
    let symlink = root.path.join("escaped-link");
    std::os::unix::fs::symlink(&escaped_target, &symlink).expect("symlink is created");

    let directory = validate_input(&policy(&root.path), &root.path);
    let escaped = validate_input(&policy(&root.path), symlink);

    assert_eq!(directory.unwrap_err().code, "input_not_regular_file");
    assert_eq!(escaped.unwrap_err().code, "input_outside_allowed_root");
}
