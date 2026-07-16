use docmorph_contracts::{
    AdapterIdentity, CapabilityDeclaration, Diagnostic, Operation, OperationKind, OperationResult,
};

/// The engine boundary. Adapters declare support before core permits invocation.
pub trait Adapter: Send + Sync {
    fn identity(&self) -> AdapterIdentity;
    fn capabilities(&self) -> Vec<CapabilityDeclaration>;
    fn execute(&self, operation: &Operation) -> Result<OperationResult, Diagnostic>;
}

/// A deterministic declaration-only adapter used to prove discovery without a document engine.
#[derive(Debug, Default)]
pub struct MockAdapter;

impl Adapter for MockAdapter {
    fn identity(&self) -> AdapterIdentity {
        AdapterIdentity {
            name: "mock".into(),
            version: "0.1.0".into(),
        }
    }

    fn capabilities(&self) -> Vec<CapabilityDeclaration> {
        vec![CapabilityDeclaration {
            operation: OperationKind::MockTransform,
            contract_major: 1,
            available: false,
        }]
    }

    fn execute(&self, operation: &Operation) -> Result<OperationResult, Diagnostic> {
        Err(Diagnostic::unavailable_operation(operation.kind))
    }
}
