use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use docmorph_contracts::{ContractVersion, ExecutionBounds, Operation, OperationKind, Provenance};
use docmorph_core::{
    Adapter, AdapterOutput, AdapterRequest, InputPolicy, Lifecycle, MockAdapter, Registry,
};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempRoot(PathBuf);

impl TempRoot {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after Unix epoch")
            .as_nanos();
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("docmorph-core-lifecycle-{unique}-{sequence}"));
        fs::create_dir(&path).expect("temporary root is created");
        Self(path)
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn operation() -> Operation {
    Operation {
        contract_version: ContractVersion::CURRENT,
        kind: OperationKind::MockTransform,
        bounds: ExecutionBounds::default(),
        provenance: Provenance {
            request_id: "request-42".into(),
            source: "integration-test".into(),
        },
    }
}

#[test]
fn accepted_input_reaches_the_mock_at_its_canonical_path_and_preserves_provenance() {
    let root = TempRoot::new();
    let input = root.0.join("input.txt");
    let destination = root.0.join("output.mock");
    fs::write(&input, b"source bytes").expect("input is written");
    let mock = Arc::new(MockAdapter::default());
    let lifecycle = Lifecycle::new(
        InputPolicy::new(vec![root.0.clone()]),
        Registry::new(vec![Arc::clone(&mock) as Arc<dyn Adapter>]),
    );

    let result = lifecycle
        .submit(&operation(), &input, &destination)
        .expect("safe input is submitted");

    assert_eq!(mock.last_input_bytes(), Some(b"source bytes".to_vec()));
    assert_eq!(fs::read(&destination).unwrap(), b"source bytes");
    assert_eq!(result.provenance, operation().provenance);
    assert_eq!(mock.invocation_count(), 1);
}

#[test]
fn policy_rejection_happens_before_the_mock_is_invoked() {
    let allowed = TempRoot::new();
    let outside = TempRoot::new();
    let input = outside.0.join("outside.txt");
    fs::write(&input, b"outside").expect("input is written");
    let mock = Arc::new(MockAdapter::default());
    let lifecycle = Lifecycle::new(
        InputPolicy::new(vec![allowed.0.clone()]),
        Registry::new(vec![Arc::clone(&mock) as Arc<dyn Adapter>]),
    );

    let error = lifecycle
        .submit(&operation(), &input, allowed.0.join("output.mock"))
        .expect_err("disallowed input is rejected");

    assert_eq!(error.code, "input_outside_allowed_root");
    assert_eq!(error.provenance, operation().provenance);
    assert_eq!(mock.invocation_count(), 0);
}

#[test]
fn disallowed_destination_is_rejected_before_the_mock_is_invoked() {
    let allowed = TempRoot::new();
    let outside = TempRoot::new();
    let input = allowed.0.join("input.txt");
    fs::write(&input, b"safe input").expect("input is written");
    let mock = Arc::new(MockAdapter::default());
    let lifecycle = Lifecycle::new(
        InputPolicy::new(vec![allowed.0.clone()]),
        Registry::new(vec![Arc::clone(&mock) as Arc<dyn Adapter>]),
    );

    let error = lifecycle
        .submit(&operation(), &input, outside.0.join("output.mock"))
        .expect_err("disallowed destination is rejected");

    assert_eq!(error.code, "output_outside_allowed_root");
    assert_eq!(error.provenance, operation().provenance);
    assert_eq!(mock.invocation_count(), 0);
}

#[test]
fn unsupported_contract_version_is_rejected_before_any_lifecycle_execution() {
    let root = TempRoot::new();
    let input = root.0.join("input.txt");
    fs::write(&input, b"safe input").expect("input is written");
    let mock = Arc::new(MockAdapter::default());
    let lifecycle = Lifecycle::new(
        InputPolicy::new(vec![root.0.clone()]),
        Registry::new(vec![Arc::clone(&mock) as Arc<dyn Adapter>]),
    );
    let unsupported = Operation {
        contract_version: ContractVersion { major: 2, minor: 0 },
        ..operation()
    };

    let error = lifecycle
        .submit(&unsupported, &input, root.0.join("output.mock"))
        .expect_err("unsupported contracts never reach lifecycle execution");

    assert_eq!(error.code, "unsupported_contract_version");
    assert_eq!(mock.invocation_count(), 0);
}

#[test]
fn unavailable_operation_never_reaches_the_registered_adapter() {
    let root = TempRoot::new();
    let input = root.0.join("input.txt");
    fs::write(&input, b"safe input").expect("input is written");
    let mock = Arc::new(MockAdapter::default());
    let lifecycle = Lifecycle::new(
        InputPolicy::new(vec![root.0.clone()]),
        Registry::new(vec![Arc::clone(&mock) as Arc<dyn Adapter>]),
    );
    let inspect = Operation {
        kind: OperationKind::Inspect,
        ..operation()
    };

    let error = lifecycle
        .submit(&inspect, &input, root.0.join("output.mock"))
        .expect_err("unavailable operations are rejected by the registry");

    assert_eq!(error.code, "operation_unavailable");
    assert_eq!(mock.invocation_count(), 0);
}

#[test]
fn registered_adapter_receives_only_validated_bytes_after_core_policy_checks() {
    let root = TempRoot::new();
    let input = root.0.join("input.txt");
    let destination = root.0.join("output.mock");
    fs::write(&input, b"validated bytes").expect("input is written");
    let adapter = Arc::new(RecordingAdapter::default());
    let lifecycle = Lifecycle::new(
        InputPolicy::new(vec![root.0.clone()]),
        Registry::new(vec![Arc::clone(&adapter) as Arc<dyn Adapter>]),
    );

    let result = lifecycle
        .submit(&operation(), &input, &destination)
        .expect("validated input reaches the registered adapter");

    assert_eq!(adapter.inputs(), vec![b"validated bytes".to_vec()]);
    assert_eq!(fs::read(&destination).unwrap(), b"validated bytes");
    assert_eq!(result.provenance, operation().provenance);
}

#[derive(Default)]
struct RecordingAdapter {
    inputs: std::sync::Mutex<Vec<Vec<u8>>>,
}

impl RecordingAdapter {
    fn inputs(&self) -> Vec<Vec<u8>> {
        self.inputs
            .lock()
            .expect("adapter state is available")
            .clone()
    }
}

impl Adapter for RecordingAdapter {
    fn identity(&self) -> docmorph_contracts::AdapterIdentity {
        docmorph_contracts::AdapterIdentity {
            name: "recording".into(),
            version: "1.0.0".into(),
        }
    }

    fn capabilities(&self) -> Vec<docmorph_contracts::CapabilityDeclaration> {
        vec![docmorph_contracts::CapabilityDeclaration {
            operation: OperationKind::MockTransform,
            contract_major: ContractVersion::CURRENT.major,
            available: true,
        }]
    }

    fn execute(
        &self,
        request: &AdapterRequest<'_>,
    ) -> Result<AdapterOutput, docmorph_contracts::Diagnostic> {
        self.inputs
            .lock()
            .expect("adapter state is available")
            .push(request.input_bytes().to_vec());
        Ok(AdapterOutput::new(request.input_bytes().to_vec()))
    }
}
