use docmorph_core::{InputPolicy, io::validate_input};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Component, Path};
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
    for fixture in &mut fixtures {
        fixture.characteristics.sort();
    }
    let canonical = serde_json::to_vec(&CorpusCatalog {
        schema_version: catalog.schema_version,
        catalog_id: catalog.catalog_id.clone(),
        fixtures,
    })
    .expect("catalog schema serializes");
    Ok(ValidatedCatalog {
        catalog_id: catalog.catalog_id,
        revision_sha256: format!("{:x}", Sha256::digest(canonical)),
    })
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
