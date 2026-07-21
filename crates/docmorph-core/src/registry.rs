use std::sync::Arc;

use docmorph_contracts::{
    AdapterIdentity, CapabilityDeclaration, Diagnostic, validate_contract_version,
};

use crate::adapter::{Adapter, AdapterOutput, AdapterRequest};

/// A deterministic view of registered adapter declarations.
#[derive(Default)]
pub struct Registry {
    adapters: Vec<Arc<dyn Adapter>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistryError {
    Contract(Diagnostic),
    Unavailable(Diagnostic),
    Adapter(Diagnostic),
}

impl<T: Adapter + 'static> From<Arc<T>> for Registry {
    fn from(adapter: Arc<T>) -> Self {
        Self::new(vec![adapter])
    }
}

impl Registry {
    #[must_use]
    pub fn new(adapters: Vec<Arc<dyn Adapter>>) -> Self {
        Self { adapters }
    }

    /// Returns identity and capabilities in a stable identity order.
    #[must_use]
    pub fn declarations(&self) -> Vec<(AdapterIdentity, Vec<CapabilityDeclaration>)> {
        let mut declarations: Vec<_> = self
            .adapters
            .iter()
            .map(|adapter| (adapter.identity(), adapter.capabilities()))
            .collect();
        declarations.sort_by(|left, right| left.0.cmp(&right.0));
        declarations
    }

    pub fn execute(&self, request: &AdapterRequest<'_>) -> Result<AdapterOutput, RegistryError> {
        let operation = request.operation();
        validate_contract_version(operation.contract_version).map_err(RegistryError::Contract)?;

        let adapter = self.adapters.iter().find(|adapter| {
            adapter.capabilities().into_iter().any(|capability| {
                capability.available
                    && capability.operation == operation.kind
                    && capability.contract_major == operation.contract_version.major
            })
        });

        let Some(adapter) = adapter else {
            return Err(RegistryError::Unavailable(
                Diagnostic::unavailable_operation(operation.kind),
            ));
        };

        adapter.execute(request).map_err(RegistryError::Adapter)
    }
}
