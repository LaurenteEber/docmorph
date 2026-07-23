use std::{ops::Deref, path::Path};

use docmorph_contracts::{Diagnostic, Operation, Provenance, validate_contract_version};

use crate::{
    InputPolicy, Registry, RegistryError,
    adapter::AdapterRequest,
    io::{Publication, validate_destination, validate_input},
};

/// Successful lifecycle output with the caller's provenance retained.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LifecycleResult {
    pub provenance: Provenance,
    pub publication: Publication,
}

/// A structured lifecycle failure that retains caller provenance for later reporting.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LifecycleFailure {
    pub diagnostic: Diagnostic,
    pub provenance: Provenance,
}

impl Deref for LifecycleFailure {
    type Target = Diagnostic;

    fn deref(&self) -> &Self::Target {
        &self.diagnostic
    }
}

/// Rust-owned ordering for local validation, adapter dispatch, and publication.
pub struct Lifecycle {
    policy: InputPolicy,
    registry: Registry,
}

impl Lifecycle {
    #[must_use]
    pub fn new(policy: InputPolicy, registry: impl Into<Registry>) -> Self {
        Self {
            policy,
            registry: registry.into(),
        }
    }

    pub fn submit(
        &self,
        operation: &Operation,
        input: impl AsRef<Path>,
        destination: impl AsRef<Path>,
    ) -> Result<LifecycleResult, LifecycleFailure> {
        validate_contract_version(operation.contract_version)
            .map_err(|diagnostic| Self::failure(operation, diagnostic))?;
        let destination = validate_destination(&self.policy, destination)
            .map_err(|diagnostic| Self::failure(operation, diagnostic))?;
        let input = validate_input(&self.policy, input)
            .map_err(|diagnostic| Self::failure(operation, diagnostic))?;
        let request = AdapterRequest::new(operation, input.bytes());
        let output = self
            .registry
            .execute(&request)
            .map_err(|error| Self::failure(operation, registry_diagnostic(error)))?;
        let publication = destination
            .publish_no_overwrite(output.bytes())
            .map_err(|diagnostic| Self::failure(operation, diagnostic))?;
        Ok(LifecycleResult {
            provenance: operation.provenance.clone(),
            publication,
        })
    }

    fn failure(operation: &Operation, diagnostic: Diagnostic) -> LifecycleFailure {
        LifecycleFailure {
            diagnostic,
            provenance: operation.provenance.clone(),
        }
    }
}

fn registry_diagnostic(error: RegistryError) -> Diagnostic {
    match error {
        RegistryError::Contract(diagnostic)
        | RegistryError::Unavailable(diagnostic)
        | RegistryError::Adapter(diagnostic) => diagnostic,
    }
}
