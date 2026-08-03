use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use docmorph_core::{Adapter as CoreAdapter, MockAdapter};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Environment {
    toolchain: Toolchain,
    build_compiler: BuildCompiler,
    platform: Platform,
    adapter: Adapter,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct Toolchain {
    rust_version: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct BuildCompiler {
    release: String,
    commit_hash: String,
    host: String,
    llvm_version: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct Platform {
    family: String,
    os: String,
    arch: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct Adapter {
    name: String,
    version: String,
}

impl Environment {
    pub fn current() -> Self {
        let adapter_identity = CoreAdapter::identity(&MockAdapter::default());
        Self {
            toolchain: Toolchain {
                rust_version: env!("CARGO_PKG_RUST_VERSION").into(),
            },
            build_compiler: BuildCompiler {
                release: env!("DOCMORPH_BUILD_RUSTC_RELEASE").into(),
                commit_hash: env!("DOCMORPH_BUILD_RUSTC_COMMIT").into(),
                host: env!("DOCMORPH_BUILD_RUSTC_HOST").into(),
                llvm_version: env!("DOCMORPH_BUILD_RUSTC_LLVM").into(),
            },
            platform: Platform {
                family: std::env::consts::FAMILY.into(),
                os: std::env::consts::OS.into(),
                arch: std::env::consts::ARCH.into(),
            },
            adapter: Adapter {
                name: adapter_identity.name,
                version: adapter_identity.version,
            },
        }
    }

    pub fn different_from(environment: &Self) -> Self {
        let mut different = environment.clone();
        different.adapter.version.push_str("-different");
        different
    }

    pub(crate) fn try_from_receipt(receipt: &serde_json::Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(serde_json::json!({
            "toolchain": receipt.get("toolchain").unwrap_or(&serde_json::Value::Null),
            "build_compiler": receipt.get("build_compiler").unwrap_or(&serde_json::Value::Null),
            "platform": receipt.get("platform").unwrap_or(&serde_json::Value::Null),
            "adapter": receipt.get("adapter").unwrap_or(&serde_json::Value::Null),
        }))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Success,
    Failure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractState {
    Satisfied,
    Violated,
    ExecutionFailed,
    EvidenceRetentionFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BaselineState {
    Match,
    Mismatch,
    IncomparableEnvironment,
    NotCompared,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OverallState {
    CompleteMatch,
    BaselineMismatch,
    IncomparableEnvironment,
    CaseContractFailure,
    CaseExecutionFailure,
    CaseEvidenceRetentionFailure,
}

#[derive(Clone, Debug, Serialize)]
pub struct CaseRecord {
    pub id: String,
    pub operation_manifest_sha256: String,
    pub declared_outcome: Outcome,
    pub observed_outcome: Outcome,
    pub expected_primary_diagnostic_code: Option<String>,
    pub observed_primary_diagnostic_code: Option<String>,
    pub declared_input_sha256: Option<String>,
    pub artifact_sha256: Option<String>,
    pub artifact_byte_len: Option<u64>,
    pub baseline_receipt_semantic_sha256: Option<String>,
    pub observed_receipt_semantic_sha256: Option<String>,
    pub contract_state: ContractState,
    pub baseline_state: BaselineState,
    pub execution_error_code: Option<String>,
    pub retention_error_code: Option<String>,
}

#[derive(Serialize)]
struct Projection<'a> {
    schema_version: &'static str,
    run_name: &'static str,
    run_definition_schema_version: &'static str,
    run_definition_sha256: &'a str,
    catalog_id: &'static str,
    catalog_execution_revision_sha256: &'a str,
    environment: &'a Environment,
    cases: &'a [CaseRecord],
    overall_state: OverallState,
}

#[derive(Serialize)]
struct PublishedReport<'a> {
    schema_version: &'static str,
    run_name: &'static str,
    run_definition_schema_version: &'static str,
    run_definition_sha256: &'a str,
    catalog_id: &'static str,
    catalog_execution_revision_sha256: &'a str,
    environment: &'a Environment,
    cases: &'a [CaseRecord],
    overall_state: OverallState,
    semantic_sha256: String,
}

pub struct Report {
    run_definition_sha256: String,
    catalog_execution_revision_sha256: String,
    environment: Environment,
    cases: Vec<CaseRecord>,
}

impl Report {
    pub fn new(
        run_definition_sha256: String,
        catalog_execution_revision_sha256: String,
        environment: Environment,
        cases: Vec<CaseRecord>,
    ) -> Self {
        Self {
            run_definition_sha256,
            catalog_execution_revision_sha256,
            environment,
            cases,
        }
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        let (overall_state, _) = resolve_outcome(&self.cases);
        serde_json::to_vec(&Projection {
            schema_version: "1.0",
            run_name: "synthetic-smoke",
            run_definition_schema_version: "1.0",
            run_definition_sha256: &self.run_definition_sha256,
            catalog_id: "docmorph-phase1-synthetic-smoke",
            catalog_execution_revision_sha256: &self.catalog_execution_revision_sha256,
            environment: &self.environment,
            cases: &self.cases,
            overall_state,
        })
    }

    pub fn semantic_sha256(&self) -> String {
        format!(
            "{:x}",
            Sha256::digest(self.canonical_bytes().expect("report is serializable"))
        )
    }

    pub fn published_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        let (overall_state, _) = resolve_outcome(&self.cases);
        serde_json::to_vec(&PublishedReport {
            schema_version: "1.0",
            run_name: "synthetic-smoke",
            run_definition_schema_version: "1.0",
            run_definition_sha256: &self.run_definition_sha256,
            catalog_id: "docmorph-phase1-synthetic-smoke",
            catalog_execution_revision_sha256: &self.catalog_execution_revision_sha256,
            environment: &self.environment,
            cases: &self.cases,
            overall_state,
            semantic_sha256: self.semantic_sha256(),
        })
    }

    pub fn exit_code(&self) -> u8 {
        resolve_outcome(&self.cases).1
    }
}

pub fn compare_case(case: &mut CaseRecord, current: &Environment, baseline: &Environment) {
    case.baseline_state = if current != baseline {
        BaselineState::IncomparableEnvironment
    } else {
        match (
            &case.baseline_receipt_semantic_sha256,
            &case.observed_receipt_semantic_sha256,
        ) {
            (Some(baseline), Some(observed)) if baseline == observed => BaselineState::Match,
            (Some(_), Some(_)) => BaselineState::Mismatch,
            _ => BaselineState::NotCompared,
        }
    };
}

pub fn evaluate_contract(
    declared: Outcome,
    observed: Outcome,
    expected_diagnostic: Option<&str>,
    observed_diagnostic: Option<&str>,
    execution_error: Option<&str>,
    retention_error: Option<&str>,
) -> ContractState {
    if retention_error.is_some() {
        ContractState::EvidenceRetentionFailed
    } else if execution_error.is_some() {
        ContractState::ExecutionFailed
    } else if declared == observed && expected_diagnostic == observed_diagnostic {
        ContractState::Satisfied
    } else {
        ContractState::Violated
    }
}

pub fn resolve_outcome(cases: &[CaseRecord]) -> (OverallState, u8) {
    for (state, overall) in [
        (
            ContractState::EvidenceRetentionFailed,
            OverallState::CaseEvidenceRetentionFailure,
        ),
        (
            ContractState::ExecutionFailed,
            OverallState::CaseExecutionFailure,
        ),
        (ContractState::Violated, OverallState::CaseContractFailure),
    ] {
        if cases.iter().any(|case| case.contract_state == state) {
            return (overall, 3);
        }
    }
    if cases
        .iter()
        .any(|case| case.baseline_state == BaselineState::IncomparableEnvironment)
    {
        return (OverallState::IncomparableEnvironment, 5);
    }
    if cases
        .iter()
        .any(|case| case.baseline_state == BaselineState::Mismatch)
    {
        (OverallState::BaselineMismatch, 4)
    } else {
        (OverallState::CompleteMatch, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(id: &str) -> CaseRecord {
        CaseRecord {
            id: id.into(),
            operation_manifest_sha256: "a".repeat(64),
            declared_outcome: Outcome::Success,
            observed_outcome: Outcome::Success,
            expected_primary_diagnostic_code: None,
            observed_primary_diagnostic_code: None,
            declared_input_sha256: Some("b".repeat(64)),
            artifact_sha256: Some("c".repeat(64)),
            artifact_byte_len: Some(37),
            baseline_receipt_semantic_sha256: Some("d".repeat(64)),
            observed_receipt_semantic_sha256: Some("d".repeat(64)),
            contract_state: ContractState::Satisfied,
            baseline_state: BaselineState::Match,
            execution_error_code: None,
            retention_error_code: None,
        }
    }

    #[test]
    fn operational_failure_outranks_incomparable() {
        let mut operational_failure = case("policy-failure");
        operational_failure.contract_state = ContractState::ExecutionFailed;
        operational_failure.execution_error_code = Some("execution_failed".into());
        let mut incomparable = case("success");
        incomparable.baseline_state = BaselineState::IncomparableEnvironment;

        assert_eq!(
            resolve_outcome(&[operational_failure, incomparable.clone()]),
            (OverallState::CaseExecutionFailure, 3)
        );

        assert_eq!(
            resolve_outcome(&[case("success"), incomparable]),
            (OverallState::IncomparableEnvironment, 5)
        );
    }
}
