//! Manifest-driven evidence receipts for deterministic local mock fixtures.

#[allow(dead_code)]
mod catalog;
mod legacy;
#[allow(dead_code)]
mod report;
#[allow(dead_code)]
mod structural_catalog;

use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::Arc,
    time::Instant,
};

use docmorph_contracts::{
    AdapterIdentity, ContractVersion, Diagnostic, ExecutionBounds, MetricAvailability, Operation,
    OperationKind, Provenance,
};
use docmorph_core::{Adapter, InputPolicy, Lifecycle, MockAdapter, Registry};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const PEAK_MEMORY_UNAVAILABLE_REASON: &str = "peak_memory_metric_not_supported";

#[derive(Deserialize)]
struct Manifest {
    contract_version: ContractVersion,
    fixtures: Vec<ManifestFixture>,
}

#[derive(Deserialize)]
struct ManifestFixture {
    id: String,
    input: PathBuf,
    output: PathBuf,
    allowed_roots: Vec<PathBuf>,
    expected_outcome: ExpectedOutcome,
    expected_diagnostic_code: Option<String>,
    provenance: Provenance,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum ExpectedOutcome {
    Success,
    Failure,
}

#[derive(Serialize)]
struct Receipt {
    schema_version: &'static str,
    catalog_id: String,
    catalog_revision_sha256: String,
    command: Vec<String>,
    manifest_sha256: String,
    contract_version: ContractVersion,
    toolchain: Toolchain,
    build_compiler: BuildCompiler,
    platform: Platform,
    adapter: AdapterIdentity,
    outcomes: Vec<FixtureOutcome>,
    elapsed_milliseconds: MetricAvailability,
    peak_memory_bytes: MetricAvailability,
    semantic_sha256: String,
}

#[derive(Serialize)]
struct SemanticReceipt<'a> {
    catalog_id: &'a str,
    catalog_revision_sha256: &'a str,
    manifest_sha256: &'a str,
    contract_version: ContractVersion,
    toolchain: &'a Toolchain,
    build_compiler: &'a BuildCompiler,
    platform: &'a Platform,
    adapter: &'a AdapterIdentity,
    outcomes: Vec<SemanticFixtureOutcome<'a>>,
    peak_memory_bytes: &'a MetricAvailability,
}

#[derive(Serialize)]
struct Toolchain {
    rust_version: String,
}

#[derive(Serialize)]
struct BuildCompiler {
    release: &'static str,
    commit_hash: &'static str,
    host: &'static str,
    llvm_version: &'static str,
}

#[derive(Serialize)]
struct Platform {
    family: String,
    os: String,
    arch: String,
}

#[derive(Serialize)]
struct FixtureOutcome {
    id: String,
    fixture_sha256: Option<String>,
    outcome: ExpectedOutcome,
    expected_diagnostic_code: Option<String>,
    diagnostics: Vec<Diagnostic>,
    artifact: Option<Artifact>,
}

#[derive(Serialize)]
struct SemanticFixtureOutcome<'a> {
    id: &'a str,
    fixture_sha256: &'a Option<String>,
    outcome: ExpectedOutcome,
    expected_diagnostic_code: &'a Option<String>,
    primary_diagnostic_code: Option<&'a str>,
    artifact_byte_len: Option<u64>,
    artifact_sha256: Option<&'a str>,
}

#[derive(Serialize)]
struct Artifact {
    path: String,
    byte_len: u64,
    sha256: String,
}

struct Arguments {
    manifest: PathBuf,
    receipt_dir: PathBuf,
    catalog: PathBuf,
    repository_root: PathBuf,
}

struct NamedArguments {
    run_name: String,
    run_root: PathBuf,
    repository_root: PathBuf,
    run_definition: PathBuf,
    catalog: PathBuf,
}

struct RunPlan {
    run_root: PathBuf,
    repository_root: PathBuf,
    catalog: PathBuf,
    run_definition_sha256: String,
    catalog_execution_revision_sha256: String,
    catalog_metadata: catalog::ValidatedCatalog,
    cases: Vec<catalog::NamedRunCase>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContractState {
    Satisfied,
    Violated,
    ExecutionFailed,
    EvidenceRetentionFailed,
}

enum CaseFailure {
    Contract(String),
    Execution(String),
    Directory(String),
    Retention(String),
}

#[allow(dead_code)]
struct NamedOutcome {
    case_states: Vec<ContractState>,
    errors: Vec<String>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("docmorph-evidence: {error}");
            ExitCode::from(match error.as_str() {
                "named_run_baseline_mismatch" => 4,
                "named_run_incomparable_environment" => 5,
                _ if error.starts_with("named_run_case_failed:")
                    || error.starts_with("named_run_report_persistence_failed:") =>
                {
                    3
                }
                _ => 2,
            })
        }
    }
}

fn run() -> Result<(), String> {
    let command = env::args().collect::<Vec<_>>();
    if command
        .iter()
        .skip(1)
        .any(|argument| argument == "--named-run")
    {
        let arguments = parse_named_arguments(command.iter().skip(1).cloned())?;
        let plan = prepare_named(&arguments)?;
        return execute_named(plan);
    }
    let arguments = parse_arguments(command.iter().skip(1).cloned())?;
    legacy::execute(command, arguments)
}

fn execute_legacy(command: Vec<String>, arguments: Arguments) -> Result<(), String> {
    let catalog_bytes =
        fs::read(&arguments.catalog).map_err(|_| "catalog_unreadable".to_owned())?;
    if !arguments.repository_root.is_dir() {
        return Err("catalog_invalid:repository_root_invalid".to_owned());
    }
    let catalog = catalog::validate_catalog_bytes(&catalog_bytes, &arguments.repository_root)
        .map_err(|errors| format!("catalog_invalid:{}", errors.codes()[0]))?;
    let started = Instant::now();
    let manifest_bytes = fs::read(&arguments.manifest)
        .map_err(|error| format!("manifest cannot be read: {error}"))?;
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| format!("manifest is invalid JSON: {error}"))?;
    validate_manifest(&manifest)?;
    let fixture_root = arguments
        .manifest
        .parent()
        .ok_or_else(|| "manifest must have a parent directory".to_owned())?;
    let artifact_root = arguments.receipt_dir.join("artifacts");
    fs::create_dir_all(&artifact_root)
        .map_err(|error| format!("receipt artifacts cannot be created: {error}"))?;

    let mock = Arc::new(MockAdapter::default());
    let adapter = mock.identity();
    let outcomes = manifest
        .fixtures
        .iter()
        .map(|fixture| run_fixture(fixture, fixture_root, &artifact_root, &manifest, &mock))
        .collect::<Result<Vec<_>, _>>()?;
    let toolchain = Toolchain {
        rust_version: env!("CARGO_PKG_RUST_VERSION").into(),
    };
    let build_compiler = BuildCompiler {
        release: env!("DOCMORPH_BUILD_RUSTC_RELEASE"),
        commit_hash: env!("DOCMORPH_BUILD_RUSTC_COMMIT"),
        host: env!("DOCMORPH_BUILD_RUSTC_HOST"),
        llvm_version: env!("DOCMORPH_BUILD_RUSTC_LLVM"),
    };
    let platform = Platform {
        family: env::consts::FAMILY.into(),
        os: env::consts::OS.into(),
        arch: env::consts::ARCH.into(),
    };
    let peak_memory_bytes = MetricAvailability::Unavailable {
        reason: PEAK_MEMORY_UNAVAILABLE_REASON.into(),
    };
    let manifest_sha256 = sha256(&manifest_bytes);
    let semantic = SemanticReceipt {
        catalog_id: &catalog.catalog_id,
        catalog_revision_sha256: &catalog.execution_revision_sha256,
        manifest_sha256: &manifest_sha256,
        contract_version: manifest.contract_version,
        toolchain: &toolchain,
        build_compiler: &build_compiler,
        platform: &platform,
        adapter: &adapter,
        outcomes: outcomes.iter().map(semantic_outcome).collect(),
        peak_memory_bytes: &peak_memory_bytes,
    };
    let semantic_sha256 = sha256(
        &serde_json::to_vec(&semantic)
            .map_err(|error| format!("receipt cannot serialize: {error}"))?,
    );
    let receipt = Receipt {
        schema_version: "1.2",
        catalog_id: catalog.catalog_id,
        catalog_revision_sha256: catalog.execution_revision_sha256,
        command,
        manifest_sha256,
        contract_version: manifest.contract_version,
        toolchain,
        build_compiler,
        platform,
        adapter,
        outcomes,
        elapsed_milliseconds: MetricAvailability::Measured {
            value: started.elapsed().as_millis() as u64,
        },
        peak_memory_bytes,
        semantic_sha256,
    };
    fs::write(
        arguments.receipt_dir.join("receipt.json"),
        serde_json::to_vec(&receipt)
            .map_err(|error| format!("receipt cannot serialize: {error}"))?,
    )
    .map_err(|error| format!("receipt cannot be retained: {error}"))
}

fn prepare_named(arguments: &NamedArguments) -> Result<RunPlan, String> {
    if arguments.run_name != "synthetic-smoke" {
        return Err(format!("named_run_unknown:{}", arguments.run_name));
    }
    let catalog_bytes =
        fs::read(&arguments.catalog).map_err(|_| "catalog_unreadable".to_owned())?;
    let catalog = catalog::validate_catalog_bytes(&catalog_bytes, &arguments.repository_root)
        .map_err(|errors| format!("catalog_invalid:{}", errors.codes()[0]))?;
    let definition = fs::read(&arguments.run_definition)
        .map_err(|_| "named_definition_unreadable".to_owned())?;
    let cases = catalog::validate_named_definition_bytes(&definition, &catalog)
        .map_err(|errors| format!("named_definition_invalid:{}", errors.codes()[0]))?;
    for case in &cases {
        let manifest_bytes = fs::read(arguments.repository_root.join(&case.operation_manifest))
            .map_err(|_| format!("named_manifest_unreadable:{}", case.id))?;
        let manifest: Manifest = serde_json::from_slice(&manifest_bytes)
            .map_err(|_| format!("named_manifest_invalid:{}", case.id))?;
        validate_manifest(&manifest).map_err(|_| format!("named_manifest_invalid:{}", case.id))?;
    }
    if arguments.run_root.exists() {
        return Err("named_run_root_already_exists".to_owned());
    }
    Ok(RunPlan {
        run_root: arguments.run_root.clone(),
        repository_root: arguments.repository_root.clone(),
        catalog: arguments.catalog.clone(),
        run_definition_sha256: sha256(&definition),
        catalog_execution_revision_sha256: catalog.execution_revision_sha256.clone(),
        catalog_metadata: catalog,
        cases,
    })
}

fn execute_named(plan: RunPlan) -> Result<(), String> {
    fs::create_dir(&plan.run_root).map_err(|_| "named_run_root_create_failed".to_owned())?;
    let cases_root = plan.run_root.join("cases");
    fs::create_dir(&cases_root).map_err(|_| "named_cases_root_create_failed".to_owned())?;
    let case_ids = plan
        .cases
        .iter()
        .map(|case| case.id.as_str())
        .collect::<Vec<_>>();
    let mut index = 0;
    let outcome = run_case_state_machine(&case_ids, |case_id| {
        let case = &plan.cases[index];
        let receipt_dir = cases_root.join(format!("{index:02}-{}", case.id));
        index += 1;
        let result =
            if case.id == "policy-failure" && named_fault() == Some("retain:policy-failure") {
                Err(CaseFailure::Retention("receipt_retention_failed".into()))
            } else {
                fs::create_dir(&receipt_dir)
                    .map_err(|_| CaseFailure::Directory("case_directory_create_failed".into()))
                    .and_then(|_| {
                        execute_legacy(
                            vec!["docmorph-evidence".into()],
                            Arguments {
                                manifest: plan.repository_root.join(&case.operation_manifest),
                                receipt_dir,
                                catalog: plan.catalog.clone(),
                                repository_root: plan.repository_root.clone(),
                            },
                        )
                        .map_err(classify_case_error)
                    })
            };
        debug_assert_eq!(case.id, case_id);
        result
    });
    let records = plan
        .cases
        .iter()
        .enumerate()
        .map(|(index, case)| named_case_record(&plan, case, index, outcome.case_states[index]))
        .collect::<Result<Vec<_>, _>>()?;
    let report = report::Report::new(
        plan.run_definition_sha256,
        plan.catalog_execution_revision_sha256,
        report::Environment::current(),
        records,
    );
    publish_report(
        &plan.run_root,
        &report
            .published_bytes()
            .map_err(|_| "report_serialize_failed")?,
    )
    .map_err(|code| format!("named_run_report_persistence_failed:{code}"))?;
    match report.exit_code() {
        0 => Ok(()),
        4 => Err("named_run_baseline_mismatch".into()),
        5 => Err("named_run_incomparable_environment".into()),
        _ => Err(format!(
            "named_run_case_failed:{}",
            outcome.errors.first().expect("failed outcome has an error")
        )),
    }
}

fn named_case_record(
    plan: &RunPlan,
    case: &catalog::NamedRunCase,
    index: usize,
    state: ContractState,
) -> Result<report::CaseRecord, String> {
    let manifest_path = plan.repository_root.join(&case.operation_manifest);
    let manifest_bytes = fs::read(&manifest_path).map_err(|_| "report_manifest_unreadable")?;
    let manifest: Manifest =
        serde_json::from_slice(&manifest_bytes).map_err(|_| "report_manifest_invalid")?;
    let fixture = manifest.fixtures.first().ok_or("report_manifest_invalid")?;
    let receipt_path = plan
        .run_root
        .join(format!("cases/{index:02}-{}", case.id))
        .join("receipt.json");
    let receipt: Option<serde_json::Value> = fs::read(&receipt_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    let observed = receipt.as_ref().and_then(|receipt| {
        receipt["outcomes"]
            .as_array()?
            .iter()
            .find(|item| item["id"] == fixture.id)
    });
    let declared_outcome = match fixture.expected_outcome {
        ExpectedOutcome::Success => report::Outcome::Success,
        ExpectedOutcome::Failure => report::Outcome::Failure,
    };
    let observed_outcome =
        observed
            .and_then(|item| item["outcome"].as_str())
            .map_or(declared_outcome, |outcome| match outcome {
                "failure" => report::Outcome::Failure,
                _ => report::Outcome::Success,
            });
    let observed_diagnostic = observed
        .and_then(|item| item["diagnostics"].as_array())
        .and_then(|items| items.first())
        .and_then(|item| item["code"].as_str())
        .map(str::to_owned);
    let retention_error = (state == ContractState::EvidenceRetentionFailed)
        .then(|| "receipt_retention_failed".to_owned());
    let contract_state = match state {
        ContractState::Satisfied => report::ContractState::Satisfied,
        ContractState::Violated => report::ContractState::Violated,
        ContractState::ExecutionFailed => report::ContractState::ExecutionFailed,
        ContractState::EvidenceRetentionFailed => report::ContractState::EvidenceRetentionFailed,
    };
    let baseline = plan.catalog_metadata.baseline(&fixture.id);
    let mut record = report::CaseRecord {
        id: case.id.clone(),
        operation_manifest_sha256: sha256(&manifest_bytes),
        declared_outcome,
        observed_outcome,
        expected_primary_diagnostic_code: fixture.expected_diagnostic_code.clone(),
        observed_primary_diagnostic_code: observed_diagnostic,
        declared_input_sha256: plan
            .catalog_metadata
            .fixture_sha256(&fixture.id)
            .map(str::to_owned),
        artifact_sha256: observed
            .and_then(|item| item["artifact"]["sha256"].as_str())
            .map(str::to_owned),
        artifact_byte_len: observed.and_then(|item| item["artifact"]["byte_len"].as_u64()),
        baseline_receipt_semantic_sha256: baseline
            .map(|baseline| baseline.semantic_sha256().into()),
        observed_receipt_semantic_sha256: receipt
            .as_ref()
            .and_then(|item| item["semantic_sha256"].as_str().map(str::to_owned)),
        contract_state,
        baseline_state: report::BaselineState::NotCompared,
        execution_error_code: None,
        retention_error_code: retention_error,
    };
    if let Some(baseline) = baseline {
        report::compare_case(
            &mut record,
            &report::Environment::current(),
            baseline.environment(),
        );
    }
    Ok(record)
}

fn publish_report(run_root: &Path, bytes: &[u8]) -> Result<(), &'static str> {
    let temporary = run_root.join(".report.json.tmp");
    if named_fault() == Some("temp-create") {
        return Err("report_temp_create_failed");
    }
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|_| "report_temp_create_failed")?;
        if named_fault() == Some("temp-write") {
            return Err("report_temp_write_failed");
        }
        file.write_all(bytes)
            .map_err(|_| "report_temp_write_failed")?;
        if named_fault() == Some("temp-flush") {
            return Err("report_temp_flush_failed");
        }
        file.flush().map_err(|_| "report_temp_flush_failed")?;
        if named_fault() == Some("rename") {
            return Err("report_rename_failed");
        }
        fs::rename(&temporary, run_root.join("report.json")).map_err(|_| "report_rename_failed")
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn named_fault() -> Option<&'static str> {
    match env::var("DOCMORPH_NAMED_FAULT").ok().as_deref() {
        Some("temp-create") => Some("temp-create"),
        Some("temp-write") => Some("temp-write"),
        Some("temp-flush") => Some("temp-flush"),
        Some("rename") => Some("rename"),
        Some("retain:policy-failure") => Some("retain:policy-failure"),
        _ => None,
    }
}

fn classify_case_error(error: String) -> CaseFailure {
    if error.starts_with("fixture `") {
        CaseFailure::Contract(error)
    } else if error.starts_with("receipt artifacts cannot") || error.starts_with("receipt cannot") {
        CaseFailure::Retention(error)
    } else {
        CaseFailure::Execution(error)
    }
}

fn run_case_state_machine(
    case_ids: &[&str],
    mut run_case: impl FnMut(&str) -> Result<(), CaseFailure>,
) -> NamedOutcome {
    let mut case_states = Vec::with_capacity(case_ids.len());
    let mut errors = Vec::new();
    for id in case_ids {
        match run_case(id) {
            Ok(()) => case_states.push(ContractState::Satisfied),
            Err(CaseFailure::Contract(error)) => {
                case_states.push(ContractState::Violated);
                errors.push(format!("{id}:{error}"));
            }
            Err(CaseFailure::Execution(error)) => {
                case_states.push(ContractState::ExecutionFailed);
                errors.push(format!("{id}:{error}"));
            }
            Err(CaseFailure::Directory(error)) => {
                case_states.push(ContractState::EvidenceRetentionFailed);
                errors.push(format!("{id}:{error}"));
            }
            Err(CaseFailure::Retention(error)) => {
                case_states.push(ContractState::EvidenceRetentionFailed);
                errors.push(format!("{id}:{error}"));
            }
        }
    }
    NamedOutcome {
        case_states,
        errors,
    }
}

fn run_fixture(
    fixture: &ManifestFixture,
    fixture_root: &Path,
    artifact_root: &Path,
    manifest: &Manifest,
    mock: &Arc<MockAdapter>,
) -> Result<FixtureOutcome, String> {
    let input = fixture_root.join(&fixture.input);
    let destination = artifact_root.join(&fixture.output);
    let mut roots = fixture
        .allowed_roots
        .iter()
        .map(|root| fixture_root.join(root))
        .collect::<Vec<_>>();
    roots.push(artifact_root.to_path_buf());
    let operation = Operation {
        contract_version: manifest.contract_version,
        kind: OperationKind::MockTransform,
        bounds: ExecutionBounds::default(),
        provenance: fixture.provenance.clone(),
    };
    let lifecycle = Lifecycle::new(
        InputPolicy::new(roots),
        Registry::new(vec![Arc::clone(mock) as Arc<dyn Adapter>]),
    );
    let result = lifecycle.submit(&operation, &input, &destination);
    let (outcome, fixture_sha256, diagnostics, artifact) = match result {
        Ok(result) => (
            ExpectedOutcome::Success,
            Some(result.publication.sha256.clone()),
            Vec::new(),
            Some(Artifact {
                path: format!("artifacts/{}", fixture.output.display()),
                byte_len: result.publication.byte_len,
                sha256: result.publication.sha256,
            }),
        ),
        Err(failure) => (
            ExpectedOutcome::Failure,
            None,
            vec![failure.diagnostic],
            None,
        ),
    };
    if std::mem::discriminant(&outcome) != std::mem::discriminant(&fixture.expected_outcome) {
        return Err(format!(
            "fixture `{}` outcome did not match its declaration",
            fixture.id
        ));
    }
    if matches!(outcome, ExpectedOutcome::Failure) {
        let actual = diagnostics
            .first()
            .map(|diagnostic| diagnostic.code.as_str())
            .ok_or_else(|| format!("fixture `{}` failure had no diagnostic", fixture.id))?;
        let expected = fixture.expected_diagnostic_code.as_deref().ok_or_else(|| {
            format!(
                "fixture `{}` failure requires expected_diagnostic_code",
                fixture.id
            )
        })?;
        if actual != expected {
            return Err(format!(
                "fixture `{}` diagnostic `{actual}` did not match expected `{expected}`",
                fixture.id
            ));
        }
    }
    Ok(FixtureOutcome {
        id: fixture.id.clone(),
        fixture_sha256,
        outcome,
        expected_diagnostic_code: fixture.expected_diagnostic_code.clone(),
        diagnostics,
        artifact,
    })
}

fn validate_manifest(manifest: &Manifest) -> Result<(), String> {
    for fixture in &manifest.fixtures {
        match (fixture.expected_outcome, &fixture.expected_diagnostic_code) {
            (ExpectedOutcome::Failure, Some(code)) if !code.is_empty() => {}
            (ExpectedOutcome::Failure, _) => {
                return Err(format!(
                    "fixture `{}` failure requires expected_diagnostic_code",
                    fixture.id
                ));
            }
            (ExpectedOutcome::Success, None) => {}
            (ExpectedOutcome::Success, _) => {
                return Err(format!(
                    "fixture `{}` success must not declare expected_diagnostic_code",
                    fixture.id
                ));
            }
        }
    }
    Ok(())
}

fn semantic_outcome(outcome: &FixtureOutcome) -> SemanticFixtureOutcome<'_> {
    SemanticFixtureOutcome {
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
    }
}

fn parse_arguments(arguments: impl Iterator<Item = String>) -> Result<Arguments, String> {
    let mut arguments = arguments;
    let mut manifest = None;
    let mut receipt_dir = None;
    let mut catalog = None;
    let mut repository_root = None;
    while let Some(argument) = arguments.next() {
        let value = arguments
            .next()
            .ok_or_else(|| format!("argument_value_missing:{argument}"))?;
        match argument.as_str() {
            "--manifest" => manifest = Some(value.into()),
            "--receipt-dir" => receipt_dir = Some(value.into()),
            "--catalog" => catalog = Some(value.into()),
            "--repository-root" => repository_root = Some(value.into()),
            _ => return Err(format!("argument_unknown:{argument}")),
        }
    }
    Ok(Arguments {
        manifest: manifest.ok_or_else(|| "argument_missing:--manifest".to_owned())?,
        receipt_dir: receipt_dir.ok_or_else(|| "argument_missing:--receipt-dir".to_owned())?,
        catalog: catalog.ok_or_else(|| "argument_missing:--catalog".to_owned())?,
        repository_root: repository_root
            .ok_or_else(|| "argument_missing:--repository-root".to_owned())?,
    })
}

fn parse_named_arguments(
    arguments: impl Iterator<Item = String>,
) -> Result<NamedArguments, String> {
    let mut arguments = arguments;
    let mut run_name = None;
    let mut run_root = None;
    let mut repository_root = None;
    let mut run_definition = None;
    let mut catalog = None;
    while let Some(argument) = arguments.next() {
        let value = arguments
            .next()
            .ok_or_else(|| format!("argument_value_missing:{argument}"))?;
        match argument.as_str() {
            "--named-run" => run_name = Some(value),
            "--run-root" => run_root = Some(value.into()),
            "--repository-root" => repository_root = Some(value.into()),
            "--run-definition" => run_definition = Some(value.into()),
            "--catalog" => catalog = Some(value.into()),
            "--manifest" | "--receipt-dir" => return Err("argument_modes_mixed".to_owned()),
            _ => return Err(format!("argument_unknown:{argument}")),
        }
    }
    let repository_root = repository_root.unwrap_or_else(|| PathBuf::from("."));
    Ok(NamedArguments {
        run_name: run_name.ok_or_else(|| "argument_missing:--named-run".to_owned())?,
        run_root: run_root.unwrap_or_else(|| PathBuf::from("evidence/synthetic-smoke-run")),
        repository_root: repository_root.clone(),
        run_definition: run_definition
            .unwrap_or_else(|| repository_root.join("fixtures/named-runs/synthetic-smoke.json")),
        catalog: catalog.unwrap_or_else(|| repository_root.join("fixtures/corpus-manifest.json")),
    })
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_machine_continues_and_applies_closed_precedence() {
        let mut attempted = Vec::new();
        let outcome = run_case_state_machine(&["policy-failure", "success"], |id| {
            attempted.push(id.to_owned());
            if id == "policy-failure" {
                Err(CaseFailure::Contract("declared_outcome_mismatch".into()))
            } else {
                Ok(())
            }
        });

        assert_eq!(attempted, ["policy-failure", "success"]);
        assert_eq!(
            outcome.case_states,
            [ContractState::Violated, ContractState::Satisfied]
        );
        assert_eq!(outcome.errors, ["policy-failure:declared_outcome_mismatch"]);

        let precedence =
            run_case_state_machine(&["contract", "execution", "directory", "retention"], |id| {
                Err(match id {
                    "contract" => CaseFailure::Contract("contract".into()),
                    "execution" => CaseFailure::Execution("execution".into()),
                    "directory" => CaseFailure::Directory("case_directory_create_failed".into()),
                    _ => CaseFailure::Retention("receipt_retention_failed".into()),
                })
            });
        assert_eq!(
            precedence.errors,
            [
                "contract:contract",
                "execution:execution",
                "directory:case_directory_create_failed",
                "retention:receipt_retention_failed"
            ]
        );
    }
}
