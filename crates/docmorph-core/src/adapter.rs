use docmorph_contracts::{AdapterIdentity, CapabilityDeclaration, Diagnostic, Operation};

/// Core-validated input exposed to an adapter without filesystem or policy authority.
pub struct AdapterRequest<'a> {
    operation: &'a Operation,
    input_bytes: &'a [u8],
}

impl<'a> AdapterRequest<'a> {
    pub(crate) fn new(operation: &'a Operation, input_bytes: &'a [u8]) -> Self {
        Self {
            operation,
            input_bytes,
        }
    }

    #[must_use]
    pub fn operation(&self) -> &Operation {
        self.operation
    }

    #[must_use]
    pub fn input_bytes(&self) -> &[u8] {
        self.input_bytes
    }
}

/// Adapter-produced bytes that remain subject to core publication and hashing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterOutput {
    bytes: Vec<u8>,
}

impl AdapterOutput {
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// The engine boundary. Adapters declare support before core permits invocation.
pub trait Adapter: Send + Sync {
    fn identity(&self) -> AdapterIdentity;
    fn capabilities(&self) -> Vec<CapabilityDeclaration>;
    fn execute(&self, request: &AdapterRequest<'_>) -> Result<AdapterOutput, Diagnostic>;
}
