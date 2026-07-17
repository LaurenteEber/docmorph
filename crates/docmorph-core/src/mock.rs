use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use docmorph_contracts::{
    AdapterIdentity, CapabilityDeclaration, Diagnostic, Operation, OperationKind, OperationResult,
};

use crate::adapter::Adapter;

/// A deterministic local adapter used to exercise lifecycle behavior without a document engine.
#[derive(Debug, Default)]
pub struct MockAdapter {
    invocations: Mutex<Vec<PathBuf>>,
}

impl MockAdapter {
    #[must_use]
    pub fn invocation_count(&self) -> usize {
        self.invocations
            .lock()
            .expect("mock state is available")
            .len()
    }

    #[must_use]
    pub fn last_input_path(&self) -> Option<PathBuf> {
        self.invocations
            .lock()
            .expect("mock state is available")
            .last()
            .cloned()
    }

    pub(crate) fn transform(&self, input: &Path, bytes: &[u8]) -> Vec<u8> {
        self.invocations
            .lock()
            .expect("mock state is available")
            .push(input.to_path_buf());
        bytes.to_vec()
    }
}

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
            available: true,
        }]
    }

    fn execute(&self, operation: &Operation) -> Result<OperationResult, Diagnostic> {
        if operation.kind != OperationKind::MockTransform {
            return Err(Diagnostic::unavailable_operation(operation.kind));
        }
        Ok(OperationResult {
            contract_version: operation.contract_version,
            diagnostics: Vec::new(),
            provenance: operation.provenance.clone(),
        })
    }
}
