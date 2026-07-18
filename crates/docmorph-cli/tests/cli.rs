use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::Value;
use sha2::{Digest, Sha256};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempRoot(PathBuf);

impl TempRoot {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after Unix epoch")
            .as_nanos();
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("docmorph-cli-{unique}-{sequence}"));
        fs::create_dir(&path).expect("temporary root is created");
        Self(path)
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_docmorph-cli"))
}

fn mock_run(args: &[&str]) -> Output {
    cli()
        .arg("mock")
        .arg("run")
        .args(args)
        .output()
        .expect("docmorph-cli binary spawns")
}

fn parse_stdout(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout is a single JSON document")
}

#[test]
fn capabilities_command_prints_the_mock_adapter_and_exits_success() {
    let output = cli().arg("capabilities").output().expect("binary spawns");

    assert_eq!(output.status.code(), Some(0));
    let json = parse_stdout(&output);
    assert_eq!(json["adapters"][0]["identity"]["name"], "mock");
    assert_eq!(
        json["adapters"][0]["capabilities"][0]["operation"],
        "mock_transform"
    );
    assert_eq!(json["adapters"][0]["capabilities"][0]["available"], true);
}

#[test]
fn unknown_subcommand_is_a_usage_error_and_exits_two() {
    let output = cli().arg("unknown").output().expect("binary spawns");

    assert_eq!(output.status.code(), Some(2));
    let json = parse_stdout(&output);
    assert_eq!(json["status"], "failure");
    assert_eq!(json["diagnostics"][0]["code"], "cli_usage_error");
}

#[test]
fn mock_run_publishes_the_input_and_exits_success() {
    let root = TempRoot::new();
    let input = root.0.join("input.txt");
    let destination = root.0.join("output.mock");
    fs::write(&input, b"cli source bytes").expect("input is written");
    let expected_sha256 = format!("{:x}", Sha256::digest(b"cli source bytes"));

    let output = mock_run(&[
        "--input",
        input.to_str().unwrap(),
        "--destination",
        destination.to_str().unwrap(),
        "--allowed-root",
        root.0.to_str().unwrap(),
        "--request-id",
        "req-1",
        "--source",
        "cli-test",
    ]);

    assert_eq!(output.status.code(), Some(0));
    let json = parse_stdout(&output);
    assert_eq!(json["status"], "success");
    assert_eq!(json["provenance"]["request_id"], "req-1");
    assert_eq!(json["provenance"]["source"], "cli-test");
    assert_eq!(json["publication"]["sha256"], expected_sha256);
    assert_eq!(json["diagnostics"].as_array().unwrap().len(), 0);
    assert_eq!(fs::read(&destination).unwrap(), b"cli source bytes");
}

#[test]
fn missing_required_input_flag_is_a_usage_error_and_exits_two() {
    let root = TempRoot::new();
    let destination = root.0.join("output.mock");

    let output = mock_run(&[
        "--destination",
        destination.to_str().unwrap(),
        "--allowed-root",
        root.0.to_str().unwrap(),
        "--request-id",
        "req-1",
        "--source",
        "cli-test",
    ]);

    assert_eq!(output.status.code(), Some(2));
    let json = parse_stdout(&output);
    assert_eq!(json["status"], "failure");
    assert_eq!(json["diagnostics"][0]["code"], "cli_usage_error");
    assert!(!destination.exists());
}

#[test]
fn unsupported_contract_major_is_a_usage_or_contract_error_and_exits_two() {
    let root = TempRoot::new();
    let input = root.0.join("input.txt");
    let destination = root.0.join("output.mock");
    fs::write(&input, b"versioned bytes").expect("input is written");

    let output = mock_run(&[
        "--input",
        input.to_str().unwrap(),
        "--destination",
        destination.to_str().unwrap(),
        "--allowed-root",
        root.0.to_str().unwrap(),
        "--request-id",
        "req-1",
        "--source",
        "cli-test",
        "--contract-major",
        "2",
    ]);

    assert_eq!(output.status.code(), Some(2));
    let json = parse_stdout(&output);
    assert_eq!(json["status"], "failure");
    assert_eq!(
        json["diagnostics"][0]["code"],
        "unsupported_contract_version"
    );
    assert_eq!(json["provenance"]["request_id"], "req-1");
    assert!(!destination.exists());
}

#[test]
fn input_outside_the_allowed_root_is_a_policy_error_and_exits_three() {
    let allowed = TempRoot::new();
    let outside = TempRoot::new();
    let input = outside.0.join("outside.txt");
    fs::write(&input, b"outside bytes").expect("input is written");
    let destination = allowed.0.join("output.mock");

    let output = mock_run(&[
        "--input",
        input.to_str().unwrap(),
        "--destination",
        destination.to_str().unwrap(),
        "--allowed-root",
        allowed.0.to_str().unwrap(),
        "--request-id",
        "req-1",
        "--source",
        "cli-test",
    ]);

    assert_eq!(output.status.code(), Some(3));
    let json = parse_stdout(&output);
    assert_eq!(json["diagnostics"][0]["code"], "input_outside_allowed_root");
    assert!(!destination.exists());
}

#[test]
fn inspect_operation_is_unsupported_by_the_mock_lifecycle_and_exits_four() {
    let root = TempRoot::new();
    let input = root.0.join("input.txt");
    let destination = root.0.join("output.mock");
    fs::write(&input, b"inspect bytes").expect("input is written");

    let output = mock_run(&[
        "--input",
        input.to_str().unwrap(),
        "--destination",
        destination.to_str().unwrap(),
        "--allowed-root",
        root.0.to_str().unwrap(),
        "--request-id",
        "req-1",
        "--source",
        "cli-test",
        "--operation",
        "inspect",
    ]);

    assert_eq!(output.status.code(), Some(4));
    let json = parse_stdout(&output);
    assert_eq!(json["diagnostics"][0]["code"], "operation_unavailable");
    assert!(!destination.exists());
}

#[test]
fn existing_destination_is_a_publication_conflict_and_exits_six() {
    let root = TempRoot::new();
    let input = root.0.join("input.txt");
    let destination = root.0.join("output.mock");
    fs::write(&input, b"fresh bytes").expect("input is written");
    fs::write(&destination, b"original bytes").expect("existing destination is written");

    let output = mock_run(&[
        "--input",
        input.to_str().unwrap(),
        "--destination",
        destination.to_str().unwrap(),
        "--allowed-root",
        root.0.to_str().unwrap(),
        "--request-id",
        "req-1",
        "--source",
        "cli-test",
    ]);

    assert_eq!(output.status.code(), Some(6));
    let json = parse_stdout(&output);
    assert_eq!(json["diagnostics"][0]["code"], "output_exists");
    assert_eq!(fs::read(&destination).unwrap(), b"original bytes");
}
