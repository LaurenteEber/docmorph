//! Versioned, engine-neutral contracts for DocMorph operations.

use serde::{Deserialize, Serialize};

/// The only schema major accepted by this foundation release.
pub const SUPPORTED_CONTRACT_MAJOR: u16 = 1;

/// A semantic version for a serialized contract envelope.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ContractVersion {
    pub major: u16,
    pub minor: u16,
}

impl ContractVersion {
    pub const CURRENT: Self = Self { major: 1, minor: 0 };

    #[must_use]
    pub const fn is_supported(self) -> bool {
        self.major == SUPPORTED_CONTRACT_MAJOR
    }
}

/// A stable machine-readable diagnostic. Its message is informational only.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
}

impl Diagnostic {
    #[must_use]
    pub fn unsupported_contract_version(version: ContractVersion) -> Self {
        Self {
            code: "unsupported_contract_version".into(),
            message: format!("contract major {} is not supported", version.major),
        }
    }

    #[must_use]
    pub fn unavailable_operation(operation: OperationKind) -> Self {
        Self {
            code: "operation_unavailable".into(),
            message: format!("operation `{}` is unavailable", operation.as_str()),
        }
    }
}

/// The operation kinds defined independently of any document engine.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Inspect,
    MockTransform,
}

impl OperationKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Inspect => "inspect",
            Self::MockTransform => "mock_transform",
        }
    }
}

/// Caller-requested execution limits. Values are declarations, not calibrated limits.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExecutionBounds {
    pub max_input_bytes: Option<u64>,
    pub timeout_seconds: Option<u64>,
}

/// Source information retained with operation results and evidence.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Provenance {
    pub request_id: String,
    pub source: String,
}

/// A request passed through the engine-neutral adapter boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Operation {
    pub contract_version: ContractVersion,
    pub kind: OperationKind,
    pub bounds: ExecutionBounds,
    pub provenance: Provenance,
}

/// The identity published by an adapter registry.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct AdapterIdentity {
    pub name: String,
    pub version: String,
}

/// An adapter's declared support for a specific operation and contract major.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CapabilityDeclaration {
    pub operation: OperationKind,
    pub contract_major: u16,
    pub available: bool,
}

/// An outcome returned from an adapter boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OperationResult {
    pub contract_version: ContractVersion,
    pub diagnostics: Vec<Diagnostic>,
    pub provenance: Provenance,
}

/// Whether a runtime metric was observed or deliberately unavailable.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum MetricAvailability {
    Measured { value: u64 },
    Unavailable { reason: String },
}

/// Evidence metadata shared by future harnesses without selecting an engine.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Receipt {
    pub contract_version: ContractVersion,
    pub adapter: AdapterIdentity,
    pub elapsed_milliseconds: MetricAvailability,
    pub peak_memory_bytes: MetricAvailability,
    pub diagnostics: Vec<Diagnostic>,
}

/// Checks a version before any operation can reach an adapter.
pub fn validate_contract_version(version: ContractVersion) -> Result<(), Diagnostic> {
    if version.is_supported() {
        Ok(())
    } else {
        Err(Diagnostic::unsupported_contract_version(version))
    }
}

/// Decodes and validates an operation envelope before it is eligible for execution.
pub fn decode_operation(json: &str) -> Result<Operation, Diagnostic> {
    let operation: Operation = serde_json::from_str(json).map_err(|error| Diagnostic {
        code: "invalid_contract".into(),
        message: error.to_string(),
    })?;
    validate_contract_version(operation.contract_version)?;
    Ok(operation)
}
