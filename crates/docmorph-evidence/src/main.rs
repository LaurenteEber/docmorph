//! Manifest-driven evidence receipts for deterministic local mock fixtures.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::Arc,
    time::Instant,
};

use docmorph_contracts::{
    AdapterIdentity, ContractVersion, Diagnostic, ExecutionBounds, MetricAvailability, Operation,
    OperationKind, Provenance,
};
use docmorph_core::{Adapter, InputPolicy, Lifecycle, MockAdapter};
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
    command: Vec<String>,
    manifest_sha256: String,
    contract_version: ContractVersion,
    toolchain: Toolchain,
    platform: Platform,
    adapter: AdapterIdentity,
    outcomes: Vec<FixtureOutcome>,
    elapsed_milliseconds: MetricAvailability,
    peak_memory_bytes: MetricAvailability,
    semantic_sha256: String,
}

#[derive(Serialize)]
struct SemanticReceipt<'a> {
    command: &'a [String],
    manifest_sha256: &'a str,
    contract_version: ContractVersion,
    toolchain: &'a Toolchain,
    platform: &'a Platform,
    adapter: &'a AdapterIdentity,
    outcomes: &'a [FixtureOutcome],
    peak_memory_bytes: &'a MetricAvailability,
}

#[derive(Serialize)]
struct Toolchain {
    rust_version: String,
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
    diagnostics: Vec<Diagnostic>,
    artifact: Option<Artifact>,
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
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("docmorph-evidence: {error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<(), String> {
    let arguments = parse_arguments(env::args().skip(1))?;
    let started = Instant::now();
    let manifest_bytes = fs::read(&arguments.manifest)
        .map_err(|error| format!("manifest cannot be read: {error}"))?;
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| format!("manifest is invalid JSON: {error}"))?;
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
    let command = vec![
        "docmorph-evidence".into(),
        "--manifest".into(),
        arguments.manifest.display().to_string(),
    ];
    let toolchain = Toolchain {
        rust_version: env!("CARGO_PKG_RUST_VERSION").into(),
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
        command: &command,
        manifest_sha256: &manifest_sha256,
        contract_version: manifest.contract_version,
        toolchain: &toolchain,
        platform: &platform,
        adapter: &adapter,
        outcomes: &outcomes,
        peak_memory_bytes: &peak_memory_bytes,
    };
    let semantic_sha256 = sha256(
        &serde_json::to_vec(&semantic)
            .map_err(|error| format!("receipt cannot serialize: {error}"))?,
    );
    let receipt = Receipt {
        command,
        manifest_sha256,
        contract_version: manifest.contract_version,
        toolchain,
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
    let lifecycle = Lifecycle::new(InputPolicy::new(roots), Arc::clone(mock));
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
    Ok(FixtureOutcome {
        id: fixture.id.clone(),
        fixture_sha256,
        outcome,
        diagnostics,
        artifact,
    })
}

fn parse_arguments(arguments: impl Iterator<Item = String>) -> Result<Arguments, String> {
    let mut arguments = arguments;
    let mut manifest = None;
    let mut receipt_dir = None;
    while let Some(argument) = arguments.next() {
        let value = arguments
            .next()
            .ok_or_else(|| format!("missing value for `{argument}`"))?;
        match argument.as_str() {
            "--manifest" => manifest = Some(value.into()),
            "--receipt-dir" => receipt_dir = Some(value.into()),
            _ => return Err(format!("unknown argument `{argument}`")),
        }
    }
    Ok(Arguments {
        manifest: manifest.ok_or_else(|| "missing `--manifest`".to_owned())?,
        receipt_dir: receipt_dir.ok_or_else(|| "missing `--receipt-dir`".to_owned())?,
    })
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
