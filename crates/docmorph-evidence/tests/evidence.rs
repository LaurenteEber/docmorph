use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

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
        let path = std::env::temp_dir().join(format!("docmorph-evidence-{unique}-{sequence}"));
        fs::create_dir(&path).expect("temporary root is created");
        Self(path)
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/manifest.json")
}

fn run(manifest_path: &std::path::Path, receipt_dir: &std::path::Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_docmorph-evidence"))
        .args([
            "--manifest",
            manifest_path.to_str().unwrap(),
            "--receipt-dir",
            receipt_dir.to_str().unwrap(),
        ])
        .output()
        .expect("evidence binary spawns")
}

fn field_value(receipt: &str, field: &str) -> String {
    let marker = format!("\"{field}\":\"");
    let value = receipt
        .split_once(&marker)
        .expect("receipt contains requested string field")
        .1;
    value
        .split_once('"')
        .expect("string field is terminated")
        .0
        .into()
}

#[test]
fn manifest_run_records_success_and_policy_failure_with_honest_metrics() {
    let root = TempRoot::new();
    let output = run(&manifest(), &root.0);

    assert_eq!(output.status.code(), Some(0));
    let receipt = fs::read_to_string(root.0.join("receipt.json")).expect("receipt is retained");
    assert!(receipt.contains("\"outcome\":\"success\""));
    assert!(receipt.contains("\"outcome\":\"failure\""));
    assert!(receipt.contains("\"code\":\"input_outside_allowed_root\""));
    assert!(receipt.contains("\"peak_memory_bytes\":{\"status\":\"unavailable\""));
    assert!(receipt.contains("\"reason\":\"peak_memory_metric_not_supported\""));
    assert!(receipt.contains("\"command\""));
    assert!(receipt.contains("\"manifest_sha256\""));
    assert!(receipt.contains("\"fixture_sha256\""));
    assert!(receipt.contains("\"adapter\":{\"name\":\"mock\""));
    let receipt_value: serde_json::Value = serde_json::from_str(&receipt).expect("receipt is JSON");
    assert_eq!(
        receipt_value["platform"]["family"],
        std::env::consts::FAMILY
    );
    assert_eq!(receipt_value["platform"]["os"], std::env::consts::OS);
    assert_eq!(receipt_value["platform"]["arch"], std::env::consts::ARCH);
    let success = receipt_value["outcomes"]
        .as_array()
        .expect("receipt outcomes are an array")
        .iter()
        .find(|outcome| outcome["id"] == "success")
        .expect("success fixture outcome is retained");
    let input = fs::read(manifest().parent().unwrap().join("mock/success-input.txt")).unwrap();
    assert_eq!(
        success["fixture_sha256"],
        format!("{:x}", Sha256::digest(input))
    );
    assert_eq!(success["artifact"]["path"], "artifacts/success-output.mock");
    assert_eq!(success["artifact"]["byte_len"], 37);
    assert_eq!(success["artifact"]["sha256"], success["fixture_sha256"]);
    assert_eq!(
        fs::read(root.0.join("artifacts/success-output.mock")).unwrap(),
        fs::read(manifest().parent().unwrap().join("mock/success-input.txt")).unwrap()
    );
    assert!(!root.0.join("artifacts/policy-failure-output.mock").exists());
}

#[test]
fn deterministic_mock_runs_keep_the_same_semantic_receipt_identity() {
    let first = TempRoot::new();
    let second = TempRoot::new();

    assert_eq!(run(&manifest(), &first.0).status.code(), Some(0));
    assert_eq!(run(&manifest(), &second.0).status.code(), Some(0));

    let first_receipt = fs::read_to_string(first.0.join("receipt.json")).unwrap();
    let second_receipt = fs::read_to_string(second.0.join("receipt.json")).unwrap();
    assert_eq!(
        field_value(&first_receipt, "semantic_sha256"),
        field_value(&second_receipt, "semantic_sha256")
    );
    let first: serde_json::Value = serde_json::from_str(&first_receipt).unwrap();
    let second: serde_json::Value = serde_json::from_str(&second_receipt).unwrap();
    assert_ne!(first["command"], second["command"]);
    assert_ne!(field_value(&first_receipt, "semantic_sha256"), "");
}

#[test]
fn receipt_command_records_each_requested_manifest_path() {
    let root = TempRoot::new();
    let manifest_contents = r#"{"contract_version":{"major":1,"minor":0},"fixtures":[]}"#;

    for name in ["success-manifest.json", "policy-failure-manifest.json"] {
        let manifest_path = root.0.join(name);
        let receipt_dir = root.0.join(format!("receipt-{name}"));
        fs::write(&manifest_path, manifest_contents).expect("alternate manifest is written");

        assert_eq!(run(&manifest_path, &receipt_dir).status.code(), Some(0));

        let receipt: serde_json::Value = serde_json::from_slice(
            &fs::read(receipt_dir.join("receipt.json")).expect("receipt is retained"),
        )
        .expect("receipt is JSON");
        assert_eq!(
            receipt["command"],
            serde_json::json!([
                env!("CARGO_BIN_EXE_docmorph-evidence"),
                "--manifest",
                manifest_path.to_string_lossy(),
                "--receipt-dir",
                receipt_dir.to_string_lossy(),
            ])
        );
        assert_eq!(receipt["schema_version"], "1.1");
        for field in ["release", "commit_hash", "host", "llvm_version"] {
            assert!(
                receipt["build_compiler"][field]
                    .as_str()
                    .is_some_and(|value| !value.is_empty()),
                "build compiler {field} is present"
            );
        }
    }
}

#[test]
fn failure_diagnostic_codes_are_required_exact_and_prevent_invalid_receipts() {
    let root = TempRoot::new();
    let allowed = root.0.join("allowed");
    fs::create_dir(&allowed).expect("allowed root is created");

    for (name, expected_code) in [
        ("missing", None),
        ("wrong", Some("input_too_large")),
        ("unexpected", Some("input_outside_allowed_root")),
    ] {
        let manifest_path = root.0.join(format!("{name}.json"));
        let receipt_dir = root.0.join(format!("receipt-{name}"));
        let expected = expected_code
            .map(|code| format!(",\"expected_diagnostic_code\":\"{code}\""))
            .unwrap_or_default();
        let expected_outcome = if name == "unexpected" {
            "success"
        } else {
            "failure"
        };
        fs::write(
            &manifest_path,
            format!(
                "{{\"contract_version\":{{\"major\":1,\"minor\":0}},\"fixtures\":[{{\"id\":\"{name}\",\"input\":\"missing.txt\",\"output\":\"result.mock\",\"allowed_roots\":[\"allowed\"],\"expected_outcome\":\"{expected_outcome}\"{expected},\"provenance\":{{\"request_id\":\"{name}\",\"source\":\"test\"}}}}]}}"
            ),
        )
        .expect("manifest is written");

        assert_ne!(run(&manifest_path, &receipt_dir).status.code(), Some(0));
        assert!(!receipt_dir.join("receipt.json").exists());
    }
}

#[test]
fn disallowed_directory_fixture_is_rejected_without_harness_read_or_hash() {
    let root = TempRoot::new();
    let allowed = root.0.join("allowed");
    let disallowed = root.0.join("disallowed-directory");
    let manifest_path = root.0.join("manifest.json");
    let receipt_dir = root.0.join("receipt");
    fs::create_dir(&allowed).expect("allowed root is created");
    fs::create_dir(&disallowed).expect("disallowed directory is created");
    fs::write(
        &manifest_path,
        r#"{"contract_version":{"major":1,"minor":0},"fixtures":[{"id":"disallowed","input":"disallowed-directory","output":"result.mock","allowed_roots":["allowed"],"expected_outcome":"failure","expected_diagnostic_code":"input_outside_allowed_root","provenance":{"request_id":"disallowed","source":"test"}}]}"#,
    )
    .expect("manifest is written");

    assert_eq!(run(&manifest_path, &receipt_dir).status.code(), Some(0));

    let receipt: serde_json::Value = serde_json::from_slice(
        &fs::read(receipt_dir.join("receipt.json")).expect("policy receipt is retained"),
    )
    .expect("receipt is JSON");
    assert_eq!(receipt["outcomes"][0]["outcome"], "failure");
    assert_eq!(
        receipt["outcomes"][0]["diagnostics"][0]["code"],
        "input_outside_allowed_root"
    );
    assert!(receipt["outcomes"][0]["fixture_sha256"].is_null());
    assert!(receipt["outcomes"][0]["artifact"].is_null());
    assert_eq!(receipt["schema_version"], "1.1");
}
