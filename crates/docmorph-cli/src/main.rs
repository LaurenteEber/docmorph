//! `docmorph-cli` — process I/O boundary for capability inspection and mock submission.
//!
//! Stable exit classes (see design "Interfaces / Contracts"):
//! `0` success, `2` usage/contract, `3` policy, `4` unsupported, `5` execution,
//! `6` publication/internal. stdout carries exactly one JSON document per
//! invocation; stderr is reserved for encoding failures only.

use std::{path::PathBuf, process::ExitCode, sync::Arc};

use docmorph_contracts::{
    AdapterIdentity, CapabilityDeclaration, ContractVersion, Diagnostic, ExecutionBounds,
    Operation, OperationKind, Provenance,
};
use docmorph_core::{InputPolicy, Lifecycle, MockAdapter, Registry};
use serde::Serialize;

const EXIT_SUCCESS: u8 = 0;
const EXIT_USAGE_OR_CONTRACT: u8 = 2;
const EXIT_POLICY: u8 = 3;
const EXIT_UNSUPPORTED: u8 = 4;
const EXIT_EXECUTION: u8 = 5;
const EXIT_PUBLICATION: u8 = 6;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("capabilities") => run_capabilities(),
        Some("mock") if args.get(1).map(String::as_str) == Some("run") => run_mock(&args[2..]),
        _ => usage_error(
            "usage: docmorph-cli capabilities | docmorph-cli mock run --input <path> \
             --destination <path> --allowed-root <path> --request-id <id> --source <source> \
             [--operation mock-transform|inspect] [--contract-major <u16>] \
             [--contract-minor <u16>]",
        ),
    }
}

#[derive(Serialize)]
struct CapabilitiesOutput {
    contract_version: ContractVersion,
    adapters: Vec<AdapterCapabilities>,
}

#[derive(Serialize)]
struct AdapterCapabilities {
    identity: AdapterIdentity,
    capabilities: Vec<CapabilityDeclaration>,
}

fn run_capabilities() -> ExitCode {
    let registry = Registry::new(vec![Arc::new(MockAdapter::default())]);
    let adapters = registry
        .declarations()
        .into_iter()
        .map(|(identity, capabilities)| AdapterCapabilities {
            identity,
            capabilities,
        })
        .collect();

    print_json(&CapabilitiesOutput {
        contract_version: ContractVersion::CURRENT,
        adapters,
    });
    ExitCode::from(EXIT_SUCCESS)
}

/// A mirror of `docmorph_core::io::Publication` with a stable JSON shape.
/// Kept local so the CLI's wire contract does not couple to core internals.
#[derive(Serialize)]
struct PublicationOutput {
    byte_len: u64,
    sha256: String,
}

impl From<docmorph_core::io::Publication> for PublicationOutput {
    fn from(publication: docmorph_core::io::Publication) -> Self {
        Self {
            byte_len: publication.byte_len,
            sha256: publication.sha256,
        }
    }
}

#[derive(Serialize)]
struct MockRunOutput {
    status: &'static str,
    contract_version: ContractVersion,
    provenance: Provenance,
    diagnostics: Vec<Diagnostic>,
    publication: Option<PublicationOutput>,
}

#[derive(Serialize)]
struct UsageErrorOutput {
    status: &'static str,
    diagnostics: Vec<Diagnostic>,
}

struct MockRunArgs {
    input: PathBuf,
    destination: PathBuf,
    allowed_roots: Vec<PathBuf>,
    request_id: String,
    source: String,
    operation: OperationKind,
    contract_version: ContractVersion,
}

fn run_mock(args: &[String]) -> ExitCode {
    let parsed = match parse_mock_run_args(args) {
        Ok(parsed) => parsed,
        Err(message) => {
            print_json(&UsageErrorOutput {
                status: "failure",
                diagnostics: vec![Diagnostic {
                    code: "cli_usage_error".into(),
                    message,
                }],
            });
            return ExitCode::from(EXIT_USAGE_OR_CONTRACT);
        }
    };

    let operation = Operation {
        contract_version: parsed.contract_version,
        kind: parsed.operation,
        bounds: ExecutionBounds::default(),
        provenance: Provenance {
            request_id: parsed.request_id,
            source: parsed.source,
        },
    };

    let policy = InputPolicy::new(parsed.allowed_roots);
    let lifecycle = Lifecycle::new(
        policy,
        Registry::new(vec![Arc::new(MockAdapter::default())]),
    );

    match lifecycle.submit(&operation, &parsed.input, &parsed.destination) {
        Ok(result) => {
            print_json(&MockRunOutput {
                status: "success",
                contract_version: operation.contract_version,
                provenance: result.provenance,
                diagnostics: Vec::new(),
                publication: Some(result.publication.into()),
            });
            ExitCode::from(EXIT_SUCCESS)
        }
        Err(failure) => {
            let exit = exit_code_for_diagnostic(&failure.diagnostic.code);
            print_json(&MockRunOutput {
                status: "failure",
                contract_version: operation.contract_version,
                provenance: failure.provenance,
                diagnostics: vec![failure.diagnostic],
                publication: None,
            });
            ExitCode::from(exit)
        }
    }
}

/// Classifies a core/contract diagnostic code into its documented exit class.
///
/// Phase 0's deterministic mock adapter has no live path that reaches a
/// genuine adapter execution failure once invoked (`Lifecycle::submit`
/// validates policy and contract before invocation, and the mock transform
/// cannot fail). Class `5` is therefore reserved here as the fallback for any
/// diagnostic code that does not belong to the usage/contract, policy,
/// unsupported, or publication families, so future adapters that report a
/// genuine execution failure are classified correctly without a CLI change.
fn exit_code_for_diagnostic(code: &str) -> u8 {
    match code {
        "unsupported_contract_version" | "invalid_contract" => EXIT_USAGE_OR_CONTRACT,
        "operation_unavailable" => EXIT_UNSUPPORTED,
        "output_exists"
        | "output_staging_failed"
        | "output_publication_failed"
        | "output_published_durability_unknown" => EXIT_PUBLICATION,
        code if code.starts_with("input_")
            || code.starts_with("output_outside")
            || code.starts_with("output_invalid") =>
        {
            EXIT_POLICY
        }
        _ => EXIT_EXECUTION,
    }
}

fn parse_mock_run_args(args: &[String]) -> Result<MockRunArgs, String> {
    let mut input = None;
    let mut destination = None;
    let mut allowed_roots = Vec::new();
    let mut request_id = None;
    let mut source = None;
    let mut operation = OperationKind::MockTransform;
    let mut contract_major = ContractVersion::CURRENT.major;
    let mut contract_minor = ContractVersion::CURRENT.minor;

    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--input" => input = Some(PathBuf::from(next_value(&mut iter, flag)?)),
            "--destination" => destination = Some(PathBuf::from(next_value(&mut iter, flag)?)),
            "--allowed-root" => allowed_roots.push(PathBuf::from(next_value(&mut iter, flag)?)),
            "--request-id" => request_id = Some(next_value(&mut iter, flag)?.clone()),
            "--source" => source = Some(next_value(&mut iter, flag)?.clone()),
            "--operation" => {
                operation = match next_value(&mut iter, flag)?.as_str() {
                    "mock-transform" => OperationKind::MockTransform,
                    "inspect" => OperationKind::Inspect,
                    other => return Err(format!("unknown `--operation` value `{other}`")),
                };
            }
            "--contract-major" => {
                contract_major = next_value(&mut iter, flag)?
                    .parse()
                    .map_err(|_| "invalid `--contract-major` value".to_string())?;
            }
            "--contract-minor" => {
                contract_minor = next_value(&mut iter, flag)?
                    .parse()
                    .map_err(|_| "invalid `--contract-minor` value".to_string())?;
            }
            other => return Err(format!("unknown flag `{other}`")),
        }
    }

    if allowed_roots.is_empty() {
        return Err("at least one `--allowed-root` is required".to_string());
    }

    Ok(MockRunArgs {
        input: input.ok_or_else(|| "missing required `--input`".to_string())?,
        destination: destination.ok_or_else(|| "missing required `--destination`".to_string())?,
        allowed_roots,
        request_id: request_id.ok_or_else(|| "missing required `--request-id`".to_string())?,
        source: source.ok_or_else(|| "missing required `--source`".to_string())?,
        operation,
        contract_version: ContractVersion {
            major: contract_major,
            minor: contract_minor,
        },
    })
}

fn next_value<'a>(
    iter: &mut std::slice::Iter<'a, String>,
    flag: &str,
) -> Result<&'a String, String> {
    iter.next()
        .ok_or_else(|| format!("missing value for `{flag}`"))
}

fn usage_error(message: &str) -> ExitCode {
    print_json(&UsageErrorOutput {
        status: "failure",
        diagnostics: vec![Diagnostic {
            code: "cli_usage_error".into(),
            message: message.into(),
        }],
    });
    ExitCode::from(EXIT_USAGE_OR_CONTRACT)
}

fn print_json<T: Serialize>(value: &T) {
    match serde_json::to_string(value) {
        Ok(json) => println!("{json}"),
        Err(error) => eprintln!(
            "{{\"status\":\"failure\",\"diagnostic\":{{\"code\":\"cli_output_encoding_failed\",\
             \"message\":\"{error}\"}}}}"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EXIT_EXECUTION, EXIT_POLICY, EXIT_PUBLICATION, EXIT_UNSUPPORTED, EXIT_USAGE_OR_CONTRACT,
        exit_code_for_diagnostic,
    };

    #[test]
    fn maps_each_diagnostic_family_to_its_documented_exit_class() {
        assert_eq!(
            exit_code_for_diagnostic("unsupported_contract_version"),
            EXIT_USAGE_OR_CONTRACT
        );
        assert_eq!(
            exit_code_for_diagnostic("input_outside_allowed_root"),
            EXIT_POLICY
        );
        assert_eq!(
            exit_code_for_diagnostic("output_invalid_destination"),
            EXIT_POLICY
        );
        assert_eq!(
            exit_code_for_diagnostic("operation_unavailable"),
            EXIT_UNSUPPORTED
        );
        assert_eq!(exit_code_for_diagnostic("output_exists"), EXIT_PUBLICATION);
        // Phase 0's deterministic mock adapter has no live execution-failure
        // path once invoked; this documents the CLI's forward-looking
        // classification contract for a future adapter-reported execution
        // diagnostic that does not match any other family.
        assert_eq!(
            exit_code_for_diagnostic("adapter_execution_failed"),
            EXIT_EXECUTION
        );
    }
}
