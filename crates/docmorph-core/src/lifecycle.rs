use std::{ops::Deref, path::Path, sync::Arc};

use docmorph_contracts::{Diagnostic, Operation, Provenance, validate_contract_version};

use crate::{
    InputPolicy,
    io::{Publication, validate_destination, validate_input},
    mock::MockAdapter,
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

/// Rust-owned ordering for local validation, mock execution, and publication.
pub struct Lifecycle {
    policy: InputPolicy,
    mock: Arc<MockAdapter>,
}

impl Lifecycle {
    #[must_use]
    pub fn new(policy: InputPolicy, mock: Arc<MockAdapter>) -> Self {
        Self { policy, mock }
    }

    pub fn submit(
        &self,
        operation: &Operation,
        input: impl AsRef<Path>,
        destination: impl AsRef<Path>,
    ) -> Result<LifecycleResult, LifecycleFailure> {
        validate_contract_version(operation.contract_version)
            .map_err(|diagnostic| Self::failure(operation, diagnostic))?;
        if operation.kind != docmorph_contracts::OperationKind::MockTransform {
            return Err(Self::failure(
                operation,
                Diagnostic::unavailable_operation(operation.kind),
            ));
        }
        let destination = validate_destination(&self.policy, destination)
            .map_err(|diagnostic| Self::failure(operation, diagnostic))?;
        let input = validate_input(&self.policy, input)
            .map_err(|diagnostic| Self::failure(operation, diagnostic))?;
        let output = self.mock.transform(input.path(), input.bytes());
        let publication = destination
            .publish_no_overwrite(&output)
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
