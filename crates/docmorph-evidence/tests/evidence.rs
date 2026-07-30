use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use sha2::{Digest, Sha256};

#[path = "../src/catalog.rs"]
mod catalog;

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

fn catalog_entry(id: &str, path: &str, sha256: &str) -> String {
    format!(
        r#"{{"id":"{id}","path":"{path}","sha256":"{sha256}","format":"mock_text","category":"harness_success_smoke","characteristics":["synthetic","smoke_only"],"provenance":{{"assertion":"project_authored_synthetic","source":"repository fixture"}},"license":{{"assertion":"workspace_license","expression":"MIT"}},"sensitivity":"synthetic_non_sensitive","distribution":"repository_allowed","comparison_intent":"byte_exact","determinism_intent":"semantic_receipt_same_environment"}}"#
    )
}

fn catalog_document(entries: &[String]) -> String {
    format!(
        r#"{{"schema_version":"1.0","catalog_id":"smoke","fixtures":[{}]}}"#,
        entries.join(",")
    )
}
fn baseline_entry(id: &str, digest: &str) -> String {
    catalog_entry(id, "fixtures/input.txt", digest).replace(
        "\"determinism_intent\":\"semantic_receipt_same_environment\"",
        &format!("\"determinism_intent\":\"semantic_receipt_same_environment\",\"baseline\":{{\"operation_manifest\":\"baseline.json\",\"retained_receipt\":\"receipt.json\",\"semantic_sha256\":\"{}\"}}", "a".repeat(64)),
    )
}

fn write_graph(root: &std::path::Path, id: &str) -> String {
    let input = root.join("fixtures/input.txt");
    fs::create_dir_all(input.parent().unwrap()).unwrap();
    fs::write(&input, b"input").unwrap();
    let manifest = format!(r#"{{"fixtures":[{{"id":"{id}","input":"fixtures/input.txt"}}]}}"#);
    fs::write(root.join("baseline.json"), &manifest).unwrap();
    fs::write(root.join("receipt.json"), format!(r#"{{"manifest_sha256":"{:x}","semantic_sha256":"{}","outcomes":[{{"id":"{id}","outcome":"success","expected_diagnostic_code":null}}]}}"#, Sha256::digest(manifest), "a".repeat(64))).unwrap();
    format!("{:x}", Sha256::digest(b"input"))
}

#[test]
fn catalog_requires_complete_safe_and_relocatable_baseline_graphs() {
    let root = TempRoot::new();
    let digest = write_graph(&root.0, "fixture");
    let document = catalog_document(&[baseline_entry("fixture", &digest)]);
    let catalog = catalog::validate_catalog_bytes(document.as_bytes(), &root.0).unwrap();
    assert!(catalog.baseline("fixture").is_some());
    assert!(catalog.baseline("unknown").is_none());
    for (paths, codes) in [
        (&["baseline.json"][..], &["baseline_manifest_missing"][..]),
        (&["receipt.json"][..], &["baseline_receipt_missing"][..]),
        (
            &["baseline.json", "receipt.json"][..],
            &["baseline_manifest_missing", "baseline_receipt_missing"][..],
        ),
    ] {
        for path in paths {
            fs::remove_file(root.0.join(path)).unwrap();
        }
        assert_eq!(
            catalog::validate_catalog_bytes(document.as_bytes(), &root.0)
                .unwrap_err()
                .codes(),
            codes
        );
        write_graph(&root.0, "fixture");
    }
    let outside = TempRoot::new();
    fs::write(outside.0.join("outside.json"), b"{}").unwrap();
    fs::remove_file(root.0.join("baseline.json")).unwrap();
    std::os::unix::fs::symlink(outside.0.join("outside.json"), root.0.join("baseline.json"))
        .unwrap();
    assert_eq!(
        catalog::validate_catalog_bytes(document.as_bytes(), &root.0)
            .unwrap_err()
            .codes(),
        ["baseline_link_escapes_repository"]
    );
    fs::remove_file(root.0.join("baseline.json")).unwrap();
    write_graph(&root.0, "fixture");
    for (path, bytes, code) in [
        (
            "baseline.json",
            b"{".as_slice(),
            "baseline_manifest_json_invalid",
        ),
        (
            "receipt.json",
            b"{".as_slice(),
            "baseline_receipt_json_invalid",
        ),
    ] {
        fs::write(root.0.join(path), bytes).unwrap();
        assert_eq!(
            catalog::validate_catalog_bytes(document.as_bytes(), &root.0)
                .unwrap_err()
                .codes(),
            [code]
        );
        write_graph(&root.0, "fixture");
    }
    fs::remove_file(root.0.join("baseline.json")).unwrap();
    fs::create_dir(root.0.join("baseline.json")).unwrap();
    assert_eq!(
        catalog::validate_catalog_bytes(document.as_bytes(), &root.0)
            .unwrap_err()
            .codes(),
        ["baseline_link_not_regular_file"]
    );
    fs::remove_dir(root.0.join("baseline.json")).unwrap();
    write_graph(&root.0, "fixture");
    let bad = format!(
        r#"{{"manifest_sha256":"{}","semantic_sha256":"{}","outcomes":[{{"id":"fixture","outcome":"failure","expected_diagnostic_code":"input_outside_allowed_root"}}]}}"#,
        "0".repeat(64),
        "a".repeat(64)
    );
    fs::write(root.0.join("receipt.json"), bad).unwrap();
    assert_eq!(
        catalog::validate_catalog_bytes(document.as_bytes(), &root.0)
            .unwrap_err()
            .codes(),
        [
            "baseline_receipt_diagnostic_mismatch",
            "baseline_receipt_outcome_mismatch",
            "receipt_manifest_sha256_mismatch"
        ]
    );
    write_graph(&root.0, "fixture");
    fs::write(root.0.join("receipt.json"), r#"{"manifest_sha256":"bad","semantic_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","outcomes":[{"id":"fixture","outcome":"success","expected_diagnostic_code":null}]}"#).unwrap();
    assert_eq!(
        catalog::validate_catalog_bytes(document.as_bytes(), &root.0)
            .unwrap_err()
            .codes(),
        ["receipt_manifest_sha256_invalid"]
    );
    let first = TempRoot::new();
    let second = TempRoot::new();
    let first_document =
        catalog_document(&[baseline_entry("fixture", &write_graph(&first.0, "fixture"))]);
    write_graph(&second.0, "fixture");
    let left = catalog::validate_catalog_bytes(first_document.as_bytes(), &first.0).unwrap();
    let right = catalog::validate_catalog_bytes(first_document.as_bytes(), &second.0).unwrap();
    assert_eq!(left.revision_sha256, right.revision_sha256);
    let expected = format!(
        "Some(ValidatedBaseline {{ _link: BaselineLink {{ operation_manifest: \"baseline.json\", retained_receipt: \"receipt.json\", semantic_sha256: \"{}\" }} }})",
        "a".repeat(64)
    );
    assert_eq!(format!("{:?}", left.baseline("fixture")), expected);
    assert_eq!(format!("{:?}", right.baseline("fixture")), expected);
    let receipt = first.0.join("receipt.json");
    fs::write(
        &receipt,
        fs::read_to_string(&receipt)
            .unwrap()
            .replace("\"outcome\":\"success\"", "\"outcome\":\"failure\"")
            .replace(
                "\"expected_diagnostic_code\":null",
                "\"expected_diagnostic_code\":\"input_outside_allowed_root\"",
            ),
    )
    .unwrap();
    assert_eq!(
        catalog::validate_catalog_bytes(first_document.as_bytes(), &first.0)
            .unwrap_err()
            .codes(),
        [
            "baseline_receipt_diagnostic_mismatch",
            "baseline_receipt_outcome_mismatch"
        ]
    );
    let untouched = catalog::validate_catalog_bytes(first_document.as_bytes(), &second.0).unwrap();
    assert_eq!(format!("{:?}", untouched.baseline("fixture")), expected);
    assert_eq!(untouched.revision_sha256, right.revision_sha256);
    let semantic = TempRoot::new();
    let document = catalog_document(&[baseline_entry(
        "fixture",
        &write_graph(&semantic.0, "fixture"),
    )]);
    let receipt = semantic.0.join("receipt.json");
    fs::write(
        &receipt,
        fs::read_to_string(&receipt)
            .unwrap()
            .replace(&"a".repeat(64), &"b".repeat(64)),
    )
    .unwrap();
    assert_eq!(
        catalog::validate_catalog_bytes(document.as_bytes(), &semantic.0)
            .unwrap_err()
            .codes(),
        ["baseline_semantic_sha256_mismatch"]
    );
    let combined = TempRoot::new();
    let document = catalog_document(&[baseline_entry(
        "fixture",
        &write_graph(&combined.0, "fixture"),
    )]);
    let receipt = combined.0.join("receipt.json");
    fs::write(
        &receipt,
        fs::read_to_string(&receipt)
            .unwrap()
            .replace(&"a".repeat(64), &"b".repeat(64))
            .replace("\"outcome\":\"success\"", "\"outcome\":\"failure\"")
            .replace(
                "\"expected_diagnostic_code\":null",
                "\"expected_diagnostic_code\":\"input_outside_allowed_root\"",
            ),
    )
    .unwrap();
    assert_eq!(
        catalog::validate_catalog_bytes(document.as_bytes(), &combined.0)
            .unwrap_err()
            .codes(),
        ["baseline_semantic_sha256_mismatch"]
    );
    let manifest = combined.0.join("baseline.json");
    fs::write(
        &manifest,
        format!("{} ", fs::read_to_string(&manifest).unwrap()),
    )
    .unwrap();
    assert_eq!(
        catalog::validate_catalog_bytes(document.as_bytes(), &combined.0)
            .unwrap_err()
            .codes(),
        [
            "baseline_semantic_sha256_mismatch",
            "receipt_manifest_sha256_mismatch"
        ]
    );
    let nested = TempRoot::new();
    let nested_input = nested.0.join("fixtures/input.txt");
    fs::create_dir_all(nested_input.parent().unwrap()).unwrap();
    fs::write(&nested_input, b"input").unwrap();
    fs::create_dir_all(nested.0.join("sub")).unwrap();
    let unrelated = r#"{"fixtures":[]}"#;
    fs::write(nested.0.join("sub/baseline.json"), unrelated).unwrap();
    fs::write(
        nested.0.join("sub/receipt.json"),
        format!(
            r#"{{"manifest_sha256":"{:x}","semantic_sha256":"{}","outcomes":[{{"id":"fixture","outcome":"success","expected_diagnostic_code":null}}]}}"#,
            Sha256::digest(unrelated),
            "a".repeat(64)
        ),
    )
    .unwrap();
    let nested_entry = catalog_entry(
        "fixture",
        "fixtures/input.txt",
        &format!("{:x}", Sha256::digest(b"input")),
    )
    .replace(
        "\"determinism_intent\":\"semantic_receipt_same_environment\"",
        &format!(
            "\"determinism_intent\":\"semantic_receipt_same_environment\",\"baseline\":{{\"operation_manifest\":\"sub/baseline.json\",\"retained_receipt\":\"sub/receipt.json\",\"semantic_sha256\":\"{}\"}}",
            "a".repeat(64)
        ),
    );
    assert_eq!(
        catalog::validate_catalog_bytes(catalog_document(&[nested_entry]).as_bytes(), &nested.0)
            .unwrap_err()
            .codes(),
        ["baseline_manifest_fixture_mismatch"]
    );
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let retained = catalog::validate_catalog_bytes(
        include_bytes!("../../../fixtures/corpus-manifest.json"),
        &repository,
    )
    .unwrap();
    assert!(
        retained.baseline("success").is_some() && retained.baseline("policy-failure").is_some()
    );
}
#[test]
fn catalog_without_baseline_claims_is_canonical_and_relocatable() {
    let root = TempRoot::new();
    let fixture = root.0.join("fixtures/mock/input.txt");
    fs::create_dir_all(fixture.parent().unwrap()).unwrap();
    fs::write(&fixture, b"input").unwrap();
    let digest = format!("{:x}", Sha256::digest(b"input"));
    let first = catalog_document(&[
        catalog_entry("a", "fixtures/mock/input.txt", &digest),
        catalog_entry("b", "fixtures/mock/input.txt", &digest),
    ]);
    let reordered = format!(
        r#"{{"fixtures":[{},{}],"catalog_id":"smoke","schema_version":"1.0"}}"#,
        catalog_entry("b", "fixtures/mock/input.txt", &digest).replace(
            "[\"synthetic\",\"smoke_only\"]",
            "[\"smoke_only\",\"synthetic\"]"
        ),
        catalog_entry("a", "fixtures/mock/input.txt", &digest),
    );
    let first = catalog::validate_catalog_bytes(first.as_bytes(), &root.0).unwrap();
    let second = catalog::validate_catalog_bytes(reordered.as_bytes(), &root.0).unwrap();
    assert_eq!(first.catalog_id, "smoke");
    assert_eq!(first.revision_sha256, second.revision_sha256);
}
#[test]
fn catalog_rejects_schema_duplicate_and_metadata_errors_in_order() {
    let root = TempRoot::new();
    let entry = catalog_entry("duplicate", "../escape", "bad")
        .replace("synthetic_non_sensitive", "restricted");
    let errors = catalog::validate_catalog_bytes(
        catalog_document(&[entry.clone(), entry]).as_bytes(),
        &root.0,
    )
    .unwrap_err();
    assert_eq!(
        errors.codes(),
        vec![
            "distribution_incompatible_with_sensitivity",
            "distribution_incompatible_with_sensitivity",
            "duplicate_fixture_id",
            "path_not_repository_relative",
            "path_not_repository_relative",
            "sha256_invalid",
            "sha256_invalid"
        ]
    );
    for (document, code) in [
        (
            r#"{"schema_version":"2.0","catalog_id":"x","fixtures":[]}"#,
            "unsupported_schema_version",
        ),
        (
            r#"{"schema_version":"1.0","catalog_id":"x","fixtures":[],"unknown":true}"#,
            "catalog_schema_invalid",
        ),
        ("{", "catalog_schema_invalid"),
    ] {
        assert_eq!(
            catalog::validate_catalog_bytes(document.as_bytes(), &root.0)
                .unwrap_err()
                .codes(),
            vec![code]
        );
    }
}
#[test]
fn catalog_confines_fixture_reads_and_checks_digest() {
    let root = TempRoot::new();
    let outside = TempRoot::new();
    let fixture = root.0.join("fixtures/mock/input.txt");
    fs::create_dir_all(fixture.parent().unwrap()).unwrap();
    fs::write(&fixture, b"input").unwrap();
    fs::create_dir(root.0.join("fixtures/mock/directory")).unwrap();
    fs::write(outside.0.join("outside.txt"), b"outside").unwrap();
    std::os::unix::fs::symlink(
        outside.0.join("outside.txt"),
        root.0.join("fixtures/mock/escape.txt"),
    )
    .unwrap();
    let digest = format!("{:x}", Sha256::digest(b"input"));
    for (path, sha256, code) in [
        (
            "fixtures/mock/missing.txt",
            digest.as_str(),
            "fixture_missing",
        ),
        (
            "fixtures/mock/directory",
            digest.as_str(),
            "fixture_not_regular_file",
        ),
        (
            "fixtures/mock/escape.txt",
            "0000000000000000000000000000000000000000000000000000000000000000",
            "path_escapes_repository",
        ),
        (
            "fixtures/mock/input.txt",
            "0000000000000000000000000000000000000000000000000000000000000000",
            "sha256_mismatch",
        ),
        ("fixtures/mock/input.txt", "00", "sha256_invalid"),
    ] {
        let error = catalog::validate_catalog_bytes(
            catalog_document(&[catalog_entry("fixture", path, sha256)]).as_bytes(),
            &root.0,
        )
        .unwrap_err();
        assert_eq!(error.codes(), vec![code]);
    }
}
#[test]
fn catalog_distinguishes_oversized_fixture_input() {
    assert_eq!(
        catalog::fixture_input_code("input_too_large"),
        "fixture_too_large"
    );
}
