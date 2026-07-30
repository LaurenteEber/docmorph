use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use docmorph_contracts::{AdapterIdentity, ContractVersion, Diagnostic, MetricAvailability};
use serde::{Deserialize, Serialize};
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
    run_with_catalog(manifest_path, receipt_dir, &catalog())
}

fn run_with_catalog(
    manifest_path: &std::path::Path,
    receipt_dir: &std::path::Path,
    catalog_path: &std::path::Path,
) -> Output {
    Command::new(env!("CARGO_BIN_EXE_docmorph-evidence"))
        .args([
            "--manifest",
            manifest_path.to_str().unwrap(),
            "--receipt-dir",
            receipt_dir.to_str().unwrap(),
            "--catalog",
            catalog_path.to_str().unwrap(),
            "--repository-root",
            repository_root().to_str().unwrap(),
        ])
        .output()
        .expect("evidence binary spawns")
}

fn governed_reproduction_semantic_hashes(
    manifest_path: &std::path::Path,
    first_receipt_dir: &std::path::Path,
    second_receipt_dir: &std::path::Path,
) -> [String; 2] {
    assert_eq!(run(manifest_path, first_receipt_dir).status.code(), Some(0));
    assert_eq!(
        run(manifest_path, second_receipt_dir).status.code(),
        Some(0)
    );
    [first_receipt_dir, second_receipt_dir].map(|receipt_dir| {
        field_value(
            &fs::read_to_string(receipt_dir.join("receipt.json")).unwrap(),
            "semantic_sha256",
        )
    })
}

fn run_arguments(arguments: &[PathBuf]) -> Output {
    let arguments = arguments
        .iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    Command::new(env!("CARGO_BIN_EXE_docmorph-evidence"))
        .args(arguments)
        .output()
        .expect("evidence binary spawns")
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn catalog() -> PathBuf {
    repository_root().join("fixtures/corpus-manifest.json")
}

fn retained_graph() -> TempRoot {
    let root = TempRoot::new();
    for path in [
        "fixtures/corpus-manifest.json",
        "fixtures/evidence-success-manifest.json",
        "fixtures/evidence-policy-failure-manifest.json",
        "fixtures/mock/success-input.txt",
        "fixtures/mock/policy-failure-input.txt",
        "evidence/success/receipt.json",
        "evidence/policy-failure/receipt.json",
    ] {
        let destination = root.0.join(path);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::copy(repository_root().join(path), destination).unwrap();
    }
    root
}

fn bind_catalog_receipts(root: &std::path::Path, document: &str) {
    let catalog: serde_json::Value = serde_json::from_str(document).unwrap();
    let catalog_id = catalog["catalog_id"].as_str().unwrap();
    let revision = catalog::execution_revision_for_catalog_bytes(document.as_bytes());
    for fixture in catalog["fixtures"].as_array().unwrap() {
        let Some(path) = fixture["baseline"]["retained_receipt"].as_str() else {
            continue;
        };
        let path = root.join(path);
        let mut receipt: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        receipt["schema_version"] = "1.2".into();
        receipt["catalog_id"] = catalog_id.into();
        receipt["catalog_revision_sha256"] = revision.clone().into();
        fs::write(path, serde_json::to_vec(&receipt).unwrap()).unwrap();
    }
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

#[derive(Clone, Deserialize)]
#[rustfmt::skip]
struct ReceiptView { catalog_id: String, catalog_revision_sha256: String, manifest_sha256: String, contract_version: ContractVersion, toolchain: Toolchain, build_compiler: BuildCompiler, platform: Platform, adapter: AdapterIdentity, outcomes: Vec<FixtureReceipt>, peak_memory_bytes: MetricAvailability }

#[derive(Clone, Deserialize, Serialize)]
struct Toolchain {
    rust_version: String,
}

#[derive(Clone, Deserialize, Serialize)]
struct BuildCompiler {
    release: String,
    commit_hash: String,
    host: String,
    llvm_version: String,
}

#[derive(Clone, Deserialize, Serialize)]
struct Platform {
    family: String,
    os: String,
    arch: String,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    Success,
    Failure,
}

#[derive(Clone, Deserialize)]
#[rustfmt::skip]
struct FixtureReceipt { id: String, fixture_sha256: Option<String>, outcome: Outcome, expected_diagnostic_code: Option<String>, diagnostics: Vec<Diagnostic>, artifact: Option<Artifact> }

#[derive(Clone, Deserialize)]
struct Artifact {
    byte_len: u64,
    sha256: String,
}

#[derive(Serialize)]
#[rustfmt::skip]
struct SemanticReceipt<'a> { catalog_id: &'a str, catalog_revision_sha256: &'a str, manifest_sha256: &'a str, contract_version: ContractVersion, toolchain: &'a Toolchain, build_compiler: &'a BuildCompiler, platform: &'a Platform, adapter: &'a AdapterIdentity, outcomes: Vec<SemanticFixtureReceipt<'a>>, peak_memory_bytes: &'a MetricAvailability }

#[derive(Serialize)]
#[rustfmt::skip]
struct SemanticFixtureReceipt<'a> { id: &'a str, fixture_sha256: &'a Option<String>, outcome: Outcome, expected_diagnostic_code: &'a Option<String>, primary_diagnostic_code: Option<&'a str>, artifact_byte_len: Option<u64>, artifact_sha256: Option<&'a str> }

fn semantic_sha256(receipt: &ReceiptView) -> String {
    let semantic = SemanticReceipt {
        catalog_id: &receipt.catalog_id,
        catalog_revision_sha256: &receipt.catalog_revision_sha256,
        manifest_sha256: &receipt.manifest_sha256,
        contract_version: receipt.contract_version,
        toolchain: &receipt.toolchain,
        build_compiler: &receipt.build_compiler,
        platform: &receipt.platform,
        adapter: &receipt.adapter,
        outcomes: receipt
            .outcomes
            .iter()
            .map(|outcome| SemanticFixtureReceipt {
                id: &outcome.id,
                fixture_sha256: &outcome.fixture_sha256,
                outcome: outcome.outcome,
                expected_diagnostic_code: &outcome.expected_diagnostic_code,
                primary_diagnostic_code: outcome
                    .diagnostics
                    .first()
                    .map(|diagnostic| diagnostic.code.as_str()),
                artifact_byte_len: outcome.artifact.as_ref().map(|artifact| artifact.byte_len),
                artifact_sha256: outcome
                    .artifact
                    .as_ref()
                    .map(|artifact| artifact.sha256.as_str()),
            })
            .collect(),
        peak_memory_bytes: &receipt.peak_memory_bytes,
    };
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&semantic).unwrap())
    )
}

#[test]
fn governed_preflight_rejects_invalid_inputs_before_output_side_effects() {
    let root = TempRoot::new();
    let manifest_path = manifest();
    let catalog_path = catalog();
    let repository_root = repository_root();
    let invalid_root = root.0.join("not-a-directory");
    let malformed_catalog = root.0.join("malformed.json");
    let baseline_invalid_catalog = root.0.join("baseline-invalid.json");
    fs::write(&invalid_root, b"not a directory").unwrap();
    fs::write(&malformed_catalog, b"{").unwrap();
    fs::create_dir_all(root.0.join("fixtures")).unwrap();
    fs::write(root.0.join("fixtures/input.txt"), b"input").unwrap();
    fs::write(
        &baseline_invalid_catalog,
        catalog_document(&[baseline_entry(
            "fixture",
            &format!("{:x}", Sha256::digest(b"input")),
        )]),
    )
    .unwrap();

    let cases: Vec<(Vec<PathBuf>, &str)> = vec![
        (
            vec![
                "--manifest".into(),
                manifest_path.clone(),
                "--receipt-dir".into(),
                root.0.join("missing-catalog"),
                "--repository-root".into(),
                repository_root.clone(),
            ],
            "argument_missing:--catalog",
        ),
        (
            vec![
                "--manifest".into(),
                manifest_path.clone(),
                "--receipt-dir".into(),
                root.0.join("missing-value"),
                "--catalog".into(),
            ],
            "argument_value_missing:--catalog",
        ),
        (
            vec![
                "--manifest".into(),
                manifest_path.clone(),
                "--receipt-dir".into(),
                root.0.join("missing-root"),
                "--catalog".into(),
                catalog_path.clone(),
            ],
            "argument_missing:--repository-root",
        ),
        (
            vec![
                "--manifest".into(),
                manifest_path.clone(),
                "--receipt-dir".into(),
                root.0.join("missing-root-value"),
                "--catalog".into(),
                catalog_path.clone(),
                "--repository-root".into(),
            ],
            "argument_value_missing:--repository-root",
        ),
        (
            vec![
                "--manifest".into(),
                manifest_path.clone(),
                "--receipt-dir".into(),
                root.0.join("unreadable-catalog"),
                "--catalog".into(),
                root.0.join("missing.json"),
                "--repository-root".into(),
                repository_root.clone(),
            ],
            "catalog_unreadable",
        ),
        (
            vec![
                "--manifest".into(),
                manifest_path.clone(),
                "--receipt-dir".into(),
                root.0.join("invalid-root"),
                "--catalog".into(),
                catalog_path.clone(),
                "--repository-root".into(),
                invalid_root.clone(),
            ],
            "catalog_invalid:repository_root_invalid",
        ),
        (
            vec![
                "--manifest".into(),
                manifest_path.clone(),
                "--receipt-dir".into(),
                root.0.join("incompatible-root"),
                "--catalog".into(),
                catalog_path.clone(),
                "--repository-root".into(),
                root.0.clone(),
            ],
            "catalog_invalid:fixture_missing",
        ),
        (
            vec![
                "--manifest".into(),
                manifest_path.clone(),
                "--receipt-dir".into(),
                root.0.join("malformed-catalog"),
                "--catalog".into(),
                malformed_catalog.clone(),
                "--repository-root".into(),
                repository_root.clone(),
            ],
            "catalog_invalid:catalog_schema_invalid",
        ),
        (
            vec![
                "--manifest".into(),
                manifest_path,
                "--receipt-dir".into(),
                root.0.join("baseline-invalid-catalog"),
                "--catalog".into(),
                baseline_invalid_catalog,
                "--repository-root".into(),
                root.0.clone(),
            ],
            "catalog_invalid:baseline_manifest_missing",
        ),
    ];

    for (arguments, key) in cases {
        let receipt_dir = &arguments[3];
        let output = run_arguments(&arguments);
        assert_eq!(output.status.code(), Some(2), "{key}");
        assert_eq!(
            String::from_utf8(output.stderr).unwrap(),
            format!("docmorph-evidence: {key}\n")
        );
        assert!(!receipt_dir.join("receipt.json").exists(), "{key}");
        assert!(!receipt_dir.join("artifacts").exists(), "{key}");
    }
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
fn governed_reproduction_matches_retained_semantic_identity() {
    let first = TempRoot::new();
    let second = TempRoot::new();
    let manifest = repository_root().join("fixtures/evidence-success-manifest.json");
    let retained = repository_root().join("evidence/success/receipt.json");

    let reproduced = governed_reproduction_semantic_hashes(&manifest, &first.0, &second.0);
    let retained = field_value(&fs::read_to_string(retained).unwrap(), "semantic_sha256");

    assert_eq!(
        reproduced[0], reproduced[1],
        "semantic comparison only; byte, path, and timestamp equality are not asserted"
    );
    assert_eq!(
        reproduced[0], retained,
        "semantic comparison only; byte, path, and timestamp equality are not asserted"
    );
}

#[test]
fn governed_receipt_binds_catalog_identity_to_semantic_hash() {
    let root = TempRoot::new();
    let mut catalog_value: serde_json::Value =
        serde_json::from_slice(&fs::read(catalog()).unwrap()).unwrap();
    for fixture in catalog_value["fixtures"].as_array_mut().unwrap() {
        fixture.as_object_mut().unwrap().remove("baseline");
    }
    let catalog_bytes = serde_json::to_vec(&catalog_value).unwrap();
    let alternate_id = root.0.join("alternate-id.json");
    let alternate_revision = root.0.join("alternate-revision.json");
    fs::write(
        &alternate_id,
        String::from_utf8(catalog_bytes.clone()).unwrap().replace(
            "\"catalog_id\": \"docmorph-phase1-synthetic-smoke\"",
            "\"catalog_id\": \"alternate\"",
        ),
    )
    .unwrap();
    fs::write(
        &alternate_revision,
        String::from_utf8(catalog_bytes.clone())
            .unwrap()
            .replace("DocMorph repository fixture", "alternate provenance"),
    )
    .unwrap();

    let mut receipts = Vec::new();
    for (name, catalog_path) in [
        ("original", catalog()),
        ("alternate-id", alternate_id),
        ("alternate-revision", alternate_revision),
    ] {
        let receipt_dir = root.0.join(name);
        assert_eq!(
            run_with_catalog(&manifest(), &receipt_dir, &catalog_path)
                .status
                .code(),
            Some(0)
        );
        let receipt: serde_json::Value =
            serde_json::from_slice(&fs::read(receipt_dir.join("receipt.json")).unwrap()).unwrap();
        let validated =
            catalog::validate_catalog_bytes(&fs::read(&catalog_path).unwrap(), &repository_root())
                .unwrap();
        assert_eq!(receipt["schema_version"], "1.2");
        assert_eq!(receipt["catalog_id"], validated.catalog_id);
        assert_eq!(
            receipt["catalog_revision_sha256"],
            validated.execution_revision_sha256
        );
        let receipt_bytes = fs::read_to_string(receipt_dir.join("receipt.json")).unwrap();
        let receipt_view: ReceiptView = serde_json::from_str(&receipt_bytes).unwrap();
        assert_eq!(
            field_value(&receipt_bytes, "semantic_sha256"),
            semantic_sha256(&receipt_view)
        );
        let mut different_catalog_id = receipt_view.clone();
        different_catalog_id.catalog_id = "independent-catalog-id".into();
        assert_ne!(
            semantic_sha256(&receipt_view),
            semantic_sha256(&different_catalog_id)
        );
        receipts.push(receipt);
    }

    assert_ne!(
        receipts[0]["semantic_sha256"],
        receipts[1]["semantic_sha256"]
    );
    assert_ne!(
        receipts[0]["semantic_sha256"],
        receipts[2]["semantic_sha256"]
    );
}

#[test]
fn retained_graph_rejects_incomplete_or_mismatched_schema_1_2_bindings() {
    #[rustfmt::skip]
    let validate = |root: &TempRoot| catalog::validate_catalog_bytes(&fs::read(root.0.join("fixtures/corpus-manifest.json")).unwrap(), &root.0);
    #[rustfmt::skip]
    let receipt = |root: &TempRoot, fixture: &str| root.0.join(format!("evidence/{fixture}/receipt.json"));

    #[rustfmt::skip]
    let cases = [("catalog_id", None, "receipt_catalog_id_missing"), ("catalog_revision_sha256", None, "receipt_catalog_revision_sha256_missing"), ("schema_version", Some("1.1".to_owned()), "receipt_schema_version_invalid"), ("catalog_id", Some("wrong-catalog".to_owned()), "receipt_catalog_id_mismatch"), ("catalog_revision_sha256", Some("0".repeat(64)), "receipt_catalog_revision_sha256_mismatch")];
    for (field, replacement, code) in cases {
        let root = retained_graph();
        let path = receipt(&root, "success");
        let mut receipt: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        if let Some(replacement) = replacement {
            receipt[field] = replacement.into();
        } else {
            receipt.as_object_mut().unwrap().remove(field);
        }
        fs::write(path, serde_json::to_vec(&receipt).unwrap()).unwrap();
        assert_eq!(validate(&root).unwrap_err().codes(), [code]);
    }

    let root = retained_graph();
    let path = root.0.join("fixtures/corpus-manifest.json");
    let mut catalog: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    catalog["fixtures"][0]["baseline"]["semantic_sha256"] = "0".repeat(64).into();
    fs::write(path, serde_json::to_vec(&catalog).unwrap()).unwrap();
    #[rustfmt::skip]
    assert_eq!(validate(&root).unwrap_err().codes(), ["baseline_semantic_sha256_mismatch"]);
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
                "--catalog",
                catalog().to_string_lossy(),
                "--repository-root",
                repository_root().to_string_lossy(),
            ])
        );
        assert_eq!(receipt["schema_version"], "1.2");
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
    assert_eq!(receipt["schema_version"], "1.2");
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
    bind_catalog_receipts(&root.0, &document);
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
        bind_catalog_receipts(&root.0, &document);
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
    bind_catalog_receipts(&root.0, &document);
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
        bind_catalog_receipts(&root.0, &document);
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
    bind_catalog_receipts(&root.0, &document);
    let receipt = root.0.join("receipt.json");
    let mut bad: serde_json::Value = serde_json::from_slice(&fs::read(&receipt).unwrap()).unwrap();
    bad["manifest_sha256"] = "0".repeat(64).into();
    bad["outcomes"][0]["outcome"] = "failure".into();
    bad["outcomes"][0]["expected_diagnostic_code"] = "input_outside_allowed_root".into();
    fs::write(&receipt, serde_json::to_vec(&bad).unwrap()).unwrap();
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
    bind_catalog_receipts(&root.0, &document);
    let receipt = root.0.join("receipt.json");
    let mut bad: serde_json::Value = serde_json::from_slice(&fs::read(&receipt).unwrap()).unwrap();
    bad["manifest_sha256"] = "bad".into();
    fs::write(&receipt, serde_json::to_vec(&bad).unwrap()).unwrap();
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
    bind_catalog_receipts(&first.0, &first_document);
    bind_catalog_receipts(&second.0, &first_document);
    let left = catalog::validate_catalog_bytes(first_document.as_bytes(), &first.0).unwrap();
    let right = catalog::validate_catalog_bytes(first_document.as_bytes(), &second.0).unwrap();
    assert_eq!(left.revision_sha256, right.revision_sha256);
    let expected_semantic_sha256 = "a".repeat(64);
    assert_eq!(
        left.baseline("fixture").unwrap().semantic_sha256(),
        expected_semantic_sha256
    );
    assert_eq!(
        right.baseline("fixture").unwrap().semantic_sha256(),
        expected_semantic_sha256
    );
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
    assert_eq!(
        untouched.baseline("fixture").unwrap().semantic_sha256(),
        expected_semantic_sha256
    );
    assert_eq!(untouched.revision_sha256, right.revision_sha256);
    let semantic = TempRoot::new();
    let document = catalog_document(&[baseline_entry(
        "fixture",
        &write_graph(&semantic.0, "fixture"),
    )]);
    bind_catalog_receipts(&semantic.0, &document);
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
    bind_catalog_receipts(&combined.0, &document);
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
    let nested_document = catalog_document(&[nested_entry]);
    bind_catalog_receipts(&nested.0, &nested_document);
    assert_eq!(
        catalog::validate_catalog_bytes(nested_document.as_bytes(), &nested.0)
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
fn execution_revision_excludes_only_validated_baseline_semantic_links() {
    let root = TempRoot::new();
    let digest = write_graph(&root.0, "fixture");
    let first_document = catalog_document(&[baseline_entry("fixture", &digest)]);
    bind_catalog_receipts(&root.0, &first_document);
    let first = catalog::validate_catalog_bytes(first_document.as_bytes(), &root.0).unwrap();

    let changed_link = first_document.replace(&"a".repeat(64), &"b".repeat(64));
    assert_eq!(
        catalog::validate_catalog_bytes(changed_link.as_bytes(), &root.0)
            .unwrap_err()
            .codes(),
        ["baseline_semantic_sha256_mismatch"]
    );

    let second_root = TempRoot::new();
    let second_digest = write_graph(&second_root.0, "fixture");
    fs::write(
        second_root.0.join("receipt.json"),
        fs::read_to_string(second_root.0.join("receipt.json"))
            .unwrap()
            .replace(&"a".repeat(64), &"b".repeat(64)),
    )
    .unwrap();
    let second_document = catalog_document(&[
        baseline_entry("fixture", &second_digest).replace(&"a".repeat(64), &"b".repeat(64))
    ]);
    bind_catalog_receipts(&second_root.0, &second_document);
    let second =
        catalog::validate_catalog_bytes(second_document.as_bytes(), &second_root.0).unwrap();
    assert_ne!(first.revision_sha256, second.revision_sha256);
    assert_eq!(
        first.execution_revision_sha256,
        second.execution_revision_sha256
    );

    for changed in [
        first_document.replace("\"catalog_id\":\"smoke\"", "\"catalog_id\":\"other\""),
        first_document.replace(
            "\"comparison_intent\":\"byte_exact\"",
            "\"comparison_intent\":\"diagnostic_exact\"",
        ),
        first_document.replace("baseline.json", "alternate.json"),
        first_document.replace("receipt.json", "alternate-receipt.json"),
    ] {
        if changed.contains("alternate.json") {
            fs::copy(root.0.join("baseline.json"), root.0.join("alternate.json")).unwrap();
        }
        if changed.contains("alternate-receipt.json") {
            fs::copy(
                root.0.join("receipt.json"),
                root.0.join("alternate-receipt.json"),
            )
            .unwrap();
        }
        bind_catalog_receipts(&root.0, &changed);
        assert_ne!(
            first.execution_revision_sha256,
            catalog::validate_catalog_bytes(changed.as_bytes(), &root.0)
                .unwrap()
                .execution_revision_sha256
        );
    }

    let unordered = catalog_document(&[
        catalog_entry("b", "fixtures/input.txt", &digest),
        catalog_entry("a", "fixtures/input.txt", &digest),
    ]);
    let reordered = unordered.replace(
        "[\"synthetic\",\"smoke_only\"]",
        "[\"smoke_only\",\"synthetic\"]",
    );
    assert_eq!(
        catalog::validate_catalog_bytes(unordered.as_bytes(), &root.0)
            .unwrap()
            .execution_revision_sha256,
        catalog::validate_catalog_bytes(reordered.as_bytes(), &root.0)
            .unwrap()
            .execution_revision_sha256
    );

    fs::write(root.0.join("fixtures/input.txt"), b"changed input").unwrap();
    let changed_content =
        first_document.replace(&digest, &format!("{:x}", Sha256::digest(b"changed input")));
    bind_catalog_receipts(&root.0, &changed_content);
    assert_ne!(
        first.execution_revision_sha256,
        catalog::validate_catalog_bytes(changed_content.as_bytes(), &root.0)
            .unwrap()
            .execution_revision_sha256
    );
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
fn catalog_reports_catalog_root_and_required_metadata_contract_codes() {
    let root = TempRoot::new();
    assert_eq!(
        catalog::validate_catalog_bytes(
            br#"{"schema_version":"1.0","catalog_id":" ","fixtures":[]}"#,
            &root.0,
        )
        .unwrap_err()
        .codes(),
        ["catalog_id_invalid"]
    );
    assert_eq!(
        catalog::validate_catalog_bytes(
            br#"{"schema_version":"1.0","catalog_id":"smoke","fixtures":[]}"#,
            &root.0.join("missing"),
        )
        .unwrap_err()
        .codes(),
        ["repository_root_invalid"]
    );

    let fixture = root.0.join("fixtures/input.txt");
    fs::create_dir_all(fixture.parent().unwrap()).unwrap();
    fs::write(&fixture, b"input").unwrap();
    let entry = catalog_entry(
        "fixture",
        "fixtures/input.txt",
        &format!("{:x}", Sha256::digest(b"input")),
    )
    .replace("repository fixture", " ");
    assert_eq!(
        catalog::validate_catalog_bytes(catalog_document(&[entry]).as_bytes(), &root.0)
            .unwrap_err()
            .codes(),
        ["required_metadata_missing"]
    );
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
