use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use docmorph_contracts::{
    AdapterIdentity, CapabilityDeclaration, ContractVersion, Diagnostic, ExecutionBounds,
    Operation, OperationKind, OperationResult, Provenance,
};
use docmorph_core::{Adapter, MockAdapter, Registry, RegistryError};

fn operation(kind: OperationKind) -> Operation {
    Operation {
        contract_version: ContractVersion::CURRENT,
        kind,
        bounds: ExecutionBounds::default(),
        provenance: Provenance::default(),
    }
}

#[test]
fn mock_declaration_is_deterministic() {
    let registry = Registry::new(vec![Arc::new(MockAdapter)]);

    let declarations = registry.declarations();
    assert_eq!(declarations.len(), 1);
    assert_eq!(declarations[0].0.name, "mock");
    assert_eq!(declarations[0].1[0].operation, OperationKind::MockTransform);
    assert!(!declarations[0].1[0].available);
}

#[test]
fn unavailable_operation_never_invokes_an_adapter() {
    let capability_calls = Arc::new(AtomicUsize::new(0));
    let execute_calls = Arc::new(AtomicUsize::new(0));
    let registry = Registry::new(vec![Arc::new(CountingAdapter {
        capability_calls,
        execute_calls: Arc::clone(&execute_calls),
    })]);

    let error = registry
        .execute(&operation(OperationKind::Inspect))
        .expect_err("inspect is absent");
    assert_eq!(
        error,
        RegistryError::Unavailable(docmorph_contracts::Diagnostic::unavailable_operation(
            OperationKind::Inspect
        ))
    );
    assert_eq!(execute_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn unsupported_contract_version_never_queries_or_invokes_an_adapter() {
    let capability_calls = Arc::new(AtomicUsize::new(0));
    let execute_calls = Arc::new(AtomicUsize::new(0));
    let registry = Registry::new(vec![Arc::new(CountingAdapter {
        capability_calls: Arc::clone(&capability_calls),
        execute_calls: Arc::clone(&execute_calls),
    })]);
    let mut unsupported = operation(OperationKind::MockTransform);
    unsupported.contract_version = ContractVersion { major: 2, minor: 0 };

    let error = registry
        .execute(&unsupported)
        .expect_err("unsupported versions are rejected before adapter lookup");
    assert_eq!(
        error,
        RegistryError::Contract(Diagnostic::unsupported_contract_version(
            unsupported.contract_version
        ))
    );
    assert_eq!(capability_calls.load(Ordering::SeqCst), 0);
    assert_eq!(execute_calls.load(Ordering::SeqCst), 0);
}

struct CountingAdapter {
    capability_calls: Arc<AtomicUsize>,
    execute_calls: Arc<AtomicUsize>,
}

impl Adapter for CountingAdapter {
    fn identity(&self) -> AdapterIdentity {
        AdapterIdentity {
            name: "counting".into(),
            version: "1.0.0".into(),
        }
    }

    fn capabilities(&self) -> Vec<CapabilityDeclaration> {
        self.capability_calls.fetch_add(1, Ordering::SeqCst);
        vec![CapabilityDeclaration {
            operation: OperationKind::MockTransform,
            contract_major: 1,
            available: true,
        }]
    }

    fn execute(
        &self,
        operation: &Operation,
    ) -> Result<OperationResult, docmorph_contracts::Diagnostic> {
        self.execute_calls.fetch_add(1, Ordering::SeqCst);
        Err(docmorph_contracts::Diagnostic::unavailable_operation(
            operation.kind,
        ))
    }
}
