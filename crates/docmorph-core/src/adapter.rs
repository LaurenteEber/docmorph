use docmorph_contracts::{
    AdapterIdentity, CapabilityDeclaration, Diagnostic, Operation, OperationResult,
};

/// The engine boundary. Adapters declare support before core permits invocation.
pub trait Adapter: Send + Sync {
    fn identity(&self) -> AdapterIdentity;
    fn capabilities(&self) -> Vec<CapabilityDeclaration>;
    fn execute(&self, operation: &Operation) -> Result<OperationResult, Diagnostic>;
}
