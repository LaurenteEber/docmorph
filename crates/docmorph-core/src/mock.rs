use std::sync::Mutex;

use docmorph_contracts::{AdapterIdentity, CapabilityDeclaration, Diagnostic, OperationKind};

use crate::adapter::{Adapter, AdapterOutput, AdapterRequest};

/// A deterministic local adapter used to exercise lifecycle behavior without a document engine.
#[derive(Debug, Default)]
pub struct MockAdapter {
    invocations: Mutex<Vec<Vec<u8>>>,
}

impl MockAdapter {
    pub const IDENTITY_NAME: &str = "mock";
    pub const IDENTITY_VERSION: &str = "0.1.0";

    #[must_use]
    pub fn invocation_count(&self) -> usize {
        self.invocations
            .lock()
            .expect("mock state is available")
            .len()
    }

    #[must_use]
    pub fn last_input_bytes(&self) -> Option<Vec<u8>> {
        self.invocations
            .lock()
            .expect("mock state is available")
            .last()
            .cloned()
    }
}

impl Adapter for MockAdapter {
    fn identity(&self) -> AdapterIdentity {
        AdapterIdentity {
            name: MockAdapter::IDENTITY_NAME.into(),
            version: MockAdapter::IDENTITY_VERSION.into(),
        }
    }

    fn capabilities(&self) -> Vec<CapabilityDeclaration> {
        vec![CapabilityDeclaration {
            operation: OperationKind::MockTransform,
            contract_major: 1,
            available: true,
        }]
    }

    fn execute(&self, request: &AdapterRequest<'_>) -> Result<AdapterOutput, Diagnostic> {
        if request.operation().kind != OperationKind::MockTransform {
            return Err(Diagnostic::unavailable_operation(request.operation().kind));
        }
        let bytes = request.input_bytes().to_vec();
        self.invocations
            .lock()
            .expect("mock state is available")
            .push(bytes.clone());
        Ok(AdapterOutput::new(bytes))
    }
}
