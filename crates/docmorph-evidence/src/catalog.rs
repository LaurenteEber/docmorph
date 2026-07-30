use docmorph_core::{InputPolicy, io::validate_input};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Component, Path},
};
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CorpusCatalog {
    schema_version: String,
    catalog_id: String,
    fixtures: Vec<CorpusFixture>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CorpusFixture {
    id: String,
    path: String,
    sha256: String,
    format: FixtureFormat,
    category: FixtureCategory,
    characteristics: Vec<FixtureCharacteristic>,
    provenance: CorpusProvenance,
    license: LicenseEvidence,
    sensitivity: Sensitivity,
    distribution: Distribution,
    comparison_intent: ComparisonIntent,
    determinism_intent: DeterminismIntent,
    baseline: Option<BaselineLink>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BaselineLink {
    operation_manifest: String,
    retained_receipt: String,
    semantic_sha256: String,
}
macro_rules! enum_schema {
    ($name:ident { $($value:ident),+ $(,)? }) => {
        #[derive(Clone, Copy, Deserialize, Serialize)]
        #[serde(rename_all = "snake_case")]
        enum $name { $($value),+ }
    };
}
enum_schema!(FixtureFormat { MockText });
enum_schema!(FixtureCategory {
    HarnessSuccessSmoke,
    InputPolicyFailureSmoke
});
#[derive(Clone, Copy, Deserialize, Serialize, Ord, PartialOrd, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum FixtureCharacteristic {
    SmokeOnly,
    Synthetic,
}
enum_schema!(ProvenanceAssertion {
    ProjectAuthoredSynthetic
});
enum_schema!(LicenseAssertion { WorkspaceLicense });
enum_schema!(ComparisonIntent {
    ByteExact,
    DiagnosticExact
});
enum_schema!(DeterminismIntent {
    SemanticReceiptSameEnvironment
});
#[derive(Clone, Copy, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Sensitivity {
    SyntheticNonSensitive,
    Restricted,
    Private,
}
enum_schema!(Distribution {
    RepositoryAllowed,
    RepositoryProhibited
});
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CorpusProvenance {
    assertion: ProvenanceAssertion,
    source: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LicenseEvidence {
    assertion: LicenseAssertion,
    expression: String,
}

#[derive(Debug)]
pub struct ValidatedCatalog {
    pub catalog_id: String,
    pub revision_sha256: String,
    pub execution_revision_sha256: String,
    baselines: BTreeMap<String, ValidatedBaseline>,
}
#[derive(Clone, Debug)]
pub(crate) struct ValidatedBaseline {
    _link: BaselineLink,
}
impl ValidatedBaseline {
    pub(crate) fn semantic_sha256(&self) -> &str {
        &self._link.semantic_sha256
    }
}
impl ValidatedCatalog {
    pub(crate) fn baseline(&self, id: &str) -> Option<&ValidatedBaseline> {
        self.baselines.get(id)
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct CatalogError {
    code: &'static str,
    fixture_id: String,
    field: &'static str,
}
#[derive(Debug)]
pub struct CatalogErrors(Vec<CatalogError>);
impl CatalogErrors {
    pub fn codes(&self) -> Vec<&'static str> {
        self.0.iter().map(|error| error.code).collect()
    }
}
pub fn validate_catalog_bytes(
    bytes: &[u8],
    repository_root: &Path,
) -> Result<ValidatedCatalog, CatalogErrors> {
    let catalog: CorpusCatalog = serde_json::from_slice(bytes)
        .map_err(|_| errors(vec![error("catalog_schema_invalid", "", "schema")]))?;
    if catalog.schema_version != "1.0" {
        return Err(errors(vec![error(
            "unsupported_schema_version",
            "",
            "schema_version",
        )]));
    }
    if catalog.catalog_id.trim().is_empty() {
        return Err(errors(vec![error("catalog_id_invalid", "", "catalog_id")]));
    }
    let root = repository_root
        .canonicalize()
        .map_err(|_| errors(vec![error("repository_root_invalid", "", "root")]))?;
    let mut fixtures = catalog.fixtures;
    fixtures.sort_by(|left, right| left.id.cmp(&right.id));
    let mut findings = Vec::new();
    for pair in fixtures.windows(2) {
        if pair[0].id == pair[1].id {
            findings.push(error("duplicate_fixture_id", &pair[0].id, "id"));
        }
    }
    for fixture in &fixtures {
        validate_fixture(fixture, &root, &mut findings);
    }
    if !findings.is_empty() {
        return Err(errors(findings));
    }
    let baselines = fixtures
        .iter()
        .filter_map(|fixture| {
            fixture.baseline.as_ref().and_then(|_| {
                baseline(fixture, &root, &mut findings)
                    .map(|link| (fixture.id.clone(), ValidatedBaseline { _link: link }))
            })
        })
        .collect();
    if !findings.is_empty() {
        return Err(errors(findings));
    }
    for fixture in &mut fixtures {
        fixture.characteristics.sort();
    }
    let canonical_catalog = CorpusCatalog {
        schema_version: catalog.schema_version,
        catalog_id: catalog.catalog_id.clone(),
        fixtures,
    };
    let canonical = serde_json::to_vec(&canonical_catalog).expect("catalog schema serializes");
    Ok(ValidatedCatalog {
        catalog_id: catalog.catalog_id,
        revision_sha256: format!("{:x}", Sha256::digest(canonical)),
        execution_revision_sha256: execution_revision(&canonical_catalog),
        baselines,
    })
}

fn execution_revision(catalog: &CorpusCatalog) -> String {
    let mut projection = serde_json::to_value(catalog).expect("catalog schema serializes");
    for fixture in projection["fixtures"]
        .as_array_mut()
        .expect("catalog fixtures serialize")
    {
        if let Some(baseline) = fixture["baseline"].as_object_mut() {
            baseline.remove("semantic_sha256");
        }
    }
    let bytes = serde_json::to_vec(&projection).expect("execution projection serializes");
    format!("{:x}", Sha256::digest(bytes))
}

fn baseline(
    fixture: &CorpusFixture,
    root: &Path,
    findings: &mut Vec<CatalogError>,
) -> Option<BaselineLink> {
    let link_data = fixture.baseline.as_ref()?;
    let manifest = link(
        root,
        &link_data.operation_manifest,
        fixture,
        "operation_manifest",
        "baseline_manifest_missing",
        findings,
    );
    let receipt = link(
        root,
        &link_data.retained_receipt,
        fixture,
        "retained_receipt",
        "baseline_receipt_missing",
        findings,
    );
    let manifest = manifest.and_then(|bytes| {
        serde_json::from_slice::<serde_json::Value>(&bytes)
            .map(|json| (bytes, json))
            .map_err(|_| {
                findings.push(error(
                    "baseline_manifest_json_invalid",
                    &fixture.id,
                    "operation_manifest",
                ))
            })
            .ok()
    });
    let receipt = receipt.and_then(|bytes| {
        serde_json::from_slice::<serde_json::Value>(&bytes)
            .map_err(|_| {
                findings.push(error(
                    "baseline_receipt_json_invalid",
                    &fixture.id,
                    "retained_receipt",
                ))
            })
            .ok()
    });
    let (manifest_bytes, manifest) = manifest?;
    let receipt = receipt?;
    let hash = receipt
        .get("manifest_sha256")
        .and_then(|value| value.as_str());
    match hash {
        Some(hash) if sha256(hash) => {
            if hash != format!("{:x}", Sha256::digest(manifest_bytes)) {
                findings.push(error(
                    "receipt_manifest_sha256_mismatch",
                    &fixture.id,
                    "retained_receipt",
                ));
            }
        }
        _ => findings.push(error(
            "receipt_manifest_sha256_invalid",
            &fixture.id,
            "retained_receipt",
        )),
    }
    let semantic_bound = receipt
        .get("semantic_sha256")
        .and_then(|value| value.as_str())
        == Some(&link_data.semantic_sha256);
    if !semantic_bound {
        findings.push(error(
            "baseline_semantic_sha256_mismatch",
            &fixture.id,
            "retained_receipt",
        ));
    }
    let input = manifest
        .get("fixtures")
        .and_then(|value| value.as_array())
        .and_then(|items| items.iter().find(|item| item["id"] == fixture.id))
        .and_then(|item| item["input"].as_str());
    let parent = Path::new(&link_data.operation_manifest)
        .parent()
        .unwrap_or(Path::new(""));
    let expected_input = Path::new(&fixture.path)
        .strip_prefix(parent)
        .ok()
        .and_then(|path| path.to_str());
    if input.is_none() || input != expected_input {
        findings.push(error(
            "baseline_manifest_fixture_mismatch",
            &fixture.id,
            "operation_manifest",
        ));
    }
    if semantic_bound {
        let outcome = receipt
            .get("outcomes")
            .and_then(|value| value.as_array())
            .and_then(|items| items.iter().find(|item| item["id"] == fixture.id));
        let expected = if matches!(fixture.category, FixtureCategory::HarnessSuccessSmoke) {
            "success"
        } else {
            "failure"
        };
        if outcome.and_then(|item| item["outcome"].as_str()) != Some(expected) {
            findings.push(error(
                "baseline_receipt_outcome_mismatch",
                &fixture.id,
                "retained_receipt",
            ));
        }
        let diagnostic = if expected == "success" {
            None
        } else {
            Some("input_outside_allowed_root")
        };
        if outcome.and_then(|item| item["expected_diagnostic_code"].as_str()) != diagnostic {
            findings.push(error(
                "baseline_receipt_diagnostic_mismatch",
                &fixture.id,
                "retained_receipt",
            ));
        }
    }
    Some(link_data.clone())
}

fn link(
    root: &Path,
    path: &str,
    fixture: &CorpusFixture,
    field: &'static str,
    missing: &'static str,
    findings: &mut Vec<CatalogError>,
) -> Option<Vec<u8>> {
    if !safe_relative(path) {
        findings.push(error(
            "baseline_link_not_repository_relative",
            &fixture.id,
            field,
        ));
        return None;
    }
    validate_input(&InputPolicy::new(vec![root.to_path_buf()]), root.join(path))
        .map(|input| input.bytes().to_vec())
        .map_err(|diagnostic| {
            findings.push(error(
                match diagnostic.code.as_str() {
                    "input_outside_allowed_root" => "baseline_link_escapes_repository",
                    "input_not_regular_file" => "baseline_link_not_regular_file",
                    _ => missing,
                },
                &fixture.id,
                field,
            ))
        })
        .ok()
}

fn validate_fixture(fixture: &CorpusFixture, root: &Path, findings: &mut Vec<CatalogError>) {
    if !safe_relative(&fixture.path) {
        findings.push(error("path_not_repository_relative", &fixture.id, "path"));
    }
    if !sha256(&fixture.sha256) {
        findings.push(error("sha256_invalid", &fixture.id, "sha256"));
    }
    if !matches!(fixture.sensitivity, Sensitivity::SyntheticNonSensitive)
        && matches!(fixture.distribution, Distribution::RepositoryAllowed)
    {
        findings.push(error(
            "distribution_incompatible_with_sensitivity",
            &fixture.id,
            "distribution",
        ));
    }
    for (field, value) in [
        ("source", &fixture.provenance.source),
        ("expression", &fixture.license.expression),
    ] {
        if value.trim().is_empty() {
            findings.push(error("required_metadata_missing", &fixture.id, field));
        }
    }
    if !safe_relative(&fixture.path) || !sha256(&fixture.sha256) {
        return;
    }
    match validate_input(
        &InputPolicy::new(vec![root.to_path_buf()]),
        root.join(&fixture.path),
    ) {
        Ok(input) if format!("{:x}", Sha256::digest(input.bytes())) != fixture.sha256 => {
            findings.push(error("sha256_mismatch", &fixture.id, "sha256"))
        }
        Ok(_) => {}
        Err(diagnostic) => findings.push(error(
            fixture_input_code(&diagnostic.code),
            &fixture.id,
            "path",
        )),
    }
}

pub(crate) fn fixture_input_code(code: &str) -> &'static str {
    match code {
        "input_not_regular_file" => "fixture_not_regular_file",
        "input_outside_allowed_root" => "path_escapes_repository",
        "input_too_large" => "fixture_too_large",
        _ => "fixture_missing",
    }
}

fn safe_relative(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && !Path::new(path).is_absolute()
        && Path::new(path)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}
fn sha256(value: &str) -> bool {
    value.len() == 64
        && value.bytes().all(|byte| {
            byte.is_ascii_digit() || (byte.is_ascii_lowercase() && byte.is_ascii_hexdigit())
        })
}
fn error(code: &'static str, fixture_id: &str, field: &'static str) -> CatalogError {
    CatalogError {
        code,
        fixture_id: fixture_id.into(),
        field,
    }
}
fn errors(mut findings: Vec<CatalogError>) -> CatalogErrors {
    findings.sort_by(|left, right| {
        (&left.fixture_id, left.field, left.code).cmp(&(&right.fixture_id, right.field, right.code))
    });
    CatalogErrors(findings)
}
