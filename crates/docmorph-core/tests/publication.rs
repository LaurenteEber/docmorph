use std::{
    fs::{self, File},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use docmorph_core::io::{InputPolicy, validate_destination};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempRoot(PathBuf);

struct ProbeFile(PathBuf);

impl ProbeFile {
    fn create(path: PathBuf) -> std::io::Result<Self> {
        File::create_new(&path)?;
        Ok(Self(path))
    }
}

impl Drop for ProbeFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

impl TempRoot {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after Unix epoch")
            .as_nanos();
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("docmorph-core-publication-{unique}-{sequence}"));
        fs::create_dir(&path).expect("temporary root is created");
        Self(path)
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn assert_no_staging_residue(root: &std::path::Path) {
    assert!(fs::read_dir(root).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".docmorph-stage.")
    }));
}

fn greatest_accepted_ascii_component_length(root: &std::path::Path) -> usize {
    let accepts = |length| ProbeFile::create(root.join("p".repeat(length))).is_ok();
    let mut accepted = 1;
    assert!(accepts(accepted), "a one-byte ASCII component is accepted");
    let mut rejected = accepted * 2;
    while accepts(rejected) {
        accepted = rejected;
        rejected = rejected
            .checked_mul(2)
            .expect("component length grows safely");
    }
    while accepted + 1 < rejected {
        let candidate = accepted + (rejected - accepted) / 2;
        if accepts(candidate) {
            accepted = candidate;
        } else {
            rejected = candidate;
        }
    }
    accepted
}

#[test]
fn atomically_publishes_complete_bytes_and_returns_their_hash() {
    let root = TempRoot::new();
    let destination = root.0.join("result.mock");

    let publication = validate_destination(&InputPolicy::new(vec![root.0.clone()]), &destination)
        .expect("destination is pinned beneath the allowed root")
        .publish_no_overwrite(b"complete mock output")
        .expect("new destination is published");

    assert_eq!(fs::read(&destination).unwrap(), b"complete mock output");
    assert_eq!(publication.byte_len, 20);
    assert_eq!(
        publication.sha256,
        "c437dd4e733866ddba0923847548e544c71a4c1fbc51ee22ddffc59a4399675b"
    );
    assert!(
        fs::read_dir(&root.0)
            .unwrap()
            .all(|entry| entry.unwrap().path() == destination)
    );
    assert_no_staging_residue(&root.0);
}

#[test]
fn preserves_an_existing_destination_without_leaving_a_partial_output() {
    let root = TempRoot::new();
    let destination = root.0.join("result.mock");
    fs::write(&destination, b"original bytes").expect("existing output is written");

    let error = validate_destination(&InputPolicy::new(vec![root.0.clone()]), &destination)
        .expect("destination is pinned beneath the allowed root")
        .publish_no_overwrite(b"replacement bytes")
        .expect_err("existing output cannot be overwritten");

    assert_eq!(error.code, "output_exists");
    assert_eq!(fs::read(&destination).unwrap(), b"original bytes");
    assert_eq!(
        fs::read_dir(&root.0).unwrap().count(),
        1,
        "no staging file remains after the collision"
    );
    assert_no_staging_residue(&root.0);
}

#[test]
fn retains_an_occupied_unowned_bounded_staging_name() {
    let root = TempRoot::new();
    let destination = root.0.join("result.mock");
    let occupied_stages: Vec<_> = (0..4)
        .map(|sequence| {
            root.0
                .join(format!(".docmorph-stage.{}.{sequence}", std::process::id()))
        })
        .collect();
    for stage in &occupied_stages {
        fs::write(stage, b"live bytes").expect("occupied staging file is pre-created");
    }

    let publication = validate_destination(&InputPolicy::new(vec![root.0.clone()]), &destination)
        .expect("destination is pinned beneath the allowed root")
        .publish_no_overwrite(b"fresh bytes")
        .expect("publication retries without deleting the occupied staging file");

    assert_eq!(publication.byte_len, 11);
    assert_eq!(fs::read(&destination).unwrap(), b"fresh bytes");
    for stage in occupied_stages {
        assert_eq!(fs::read(stage).unwrap(), b"live bytes");
    }
}

#[test]
fn publishes_to_a_distinct_absent_component_at_the_filesystem_limit() {
    let root = TempRoot::new();
    let component_length = greatest_accepted_ascii_component_length(&root.0);
    let destination = root.0.join("d".repeat(component_length));
    let bytes = b"near-limit complete output";

    validate_destination(&InputPolicy::new(vec![root.0.clone()]), &destination)
        .expect("near-limit destination is pinned beneath the allowed root")
        .publish_no_overwrite(bytes)
        .expect("a legal near-limit component is published");

    assert_eq!(fs::read(&destination).unwrap(), bytes);
    assert_no_staging_residue(&root.0);
}

#[test]
fn pinned_destination_parent_cannot_be_redirected_by_a_symlink_replacement() {
    let root = TempRoot::new();
    let outside = TempRoot::new();
    let publish_directory = root.0.join("publish");
    fs::create_dir(&publish_directory).expect("publish directory is created");
    let destination = publish_directory.join("result.mock");

    let pinned = validate_destination(&InputPolicy::new(vec![root.0.clone()]), &destination)
        .expect("destination is pinned beneath the allowed root");
    let held_directory = root.0.join("publish-held");
    fs::rename(&publish_directory, &held_directory).expect("original parent is moved aside");
    std::os::unix::fs::symlink(&outside.0, &publish_directory)
        .expect("path parent is replaced by an escape symlink");

    let publication = pinned
        .publish_no_overwrite(b"pinned bytes")
        .expect("publication uses the pinned parent descriptor");

    assert_eq!(publication.byte_len, 12);
    assert_eq!(
        fs::read(held_directory.join("result.mock")).unwrap(),
        b"pinned bytes"
    );
    assert!(!outside.0.join("result.mock").exists());
}
