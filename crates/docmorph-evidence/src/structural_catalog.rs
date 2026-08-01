use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Component, Path},
};

const MAX_SOURCE_BYTES: u64 = 1_048_576;
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StructuralCatalog {
    schema_version: String,
    catalog_id: String,
    sources: Vec<Source>,
    cases: Vec<Case>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Source {
    id: String,
    path: Option<String>,
    sha256: Option<String>,
    authoring_path: String,
    authoring_sha256: String,
    provenance_path: Option<String>,
    license_path: Option<String>,
    distribution_path: Option<String>,
    metadata_path: Option<String>,
    pages: Vec<Page>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Page {
    id: String,
    index: Option<u32>,
    geometry: Option<[u32; 2]>,
    rotation_degrees: Option<u16>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    output: String,
    references: Vec<PageRef>,
    #[serde(default)]
    operation: Option<Operation>,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct PageRef {
    source_id: String,
    page_id: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Operation {
    Merge {
        inputs: Vec<Vec<PageRef>>,
        observation: Observation,
    },
    Split {
        selection: Vec<PageRef>,
        partitions: Vec<Partition>,
    },
    Rotate {
        selection: Vec<PageRef>,
        rotation_degrees: u16,
        observation: Observation,
    },
    Reorder {
        selection: Vec<PageRef>,
        permutation: Vec<PageRef>,
        observation: Observation,
    },
    DeleteSelectedPages {
        selection: Vec<PageRef>,
        removals: Vec<PageRef>,
        observation: Observation,
    },
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Partition {
    name: String,
    pages: Vec<PageRef>,
    observation: Observation,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Observation {
    page_count: usize,
    pages: Vec<ObservedPage>,
    #[serde(default)]
    baseline: Option<serde_json::Value>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ObservedPage {
    origin: PageRef,
    geometry: [u32; 2],
    effective_rotation_degrees: u16,
}

#[derive(Debug)]
pub(crate) struct StructuralCatalogErrors(Vec<&'static str>);

impl StructuralCatalogErrors {
    pub(crate) fn codes(&self) -> Vec<&'static str> {
        self.0.clone()
    }
}

pub(crate) fn validate_structural_catalog_bytes(
    bytes: &[u8],
) -> Result<(String, String), StructuralCatalogErrors> {
    let mut catalog: StructuralCatalog = serde_json::from_slice(bytes)
        .map_err(|_| StructuralCatalogErrors(vec!["structural_catalog_schema_invalid"]))?;
    if catalog.schema_version != "2.0" {
        return Err(StructuralCatalogErrors(vec!["unsupported_schema_version"]));
    }
    let mut errors = Vec::new();
    if !canonical_id(&catalog.catalog_id) {
        errors.push("catalog_id_invalid");
    }
    catalog
        .sources
        .sort_by(|left, right| left.id.cmp(&right.id));
    catalog.cases.sort_by(|left, right| left.id.cmp(&right.id));
    duplicate_ids(
        &catalog.sources,
        |source| &source.id,
        "duplicate_source_id",
        &mut errors,
    );
    for source in &mut catalog.sources {
        source.pages.sort_by(|left, right| left.id.cmp(&right.id));
        duplicate_ids(
            &source.pages,
            |page| &page.id,
            "duplicate_page_id",
            &mut errors,
        );
    }
    duplicate_ids(
        &catalog.cases,
        |case| &case.id,
        "duplicate_case_id",
        &mut errors,
    );
    let mut outputs = catalog.cases.iter().collect::<Vec<_>>();
    outputs.sort_by(|left, right| left.output.cmp(&right.output));
    duplicate_ids(
        &outputs,
        |case| &case.output,
        "duplicate_output_id",
        &mut errors,
    );
    for case in &catalog.cases {
        for reference in &case.references {
            let matches = catalog
                .sources
                .iter()
                .filter(|source| source.id == reference.source_id)
                .flat_map(|source| source.pages.iter())
                .filter(|page| page.id == reference.page_id)
                .count();
            errors.push(match matches {
                0 => "dangling_page_ref",
                1 => continue,
                _ => "ambiguous_page_ref",
            });
        }
    }
    for case in &catalog.cases {
        if let Some(operation) = &case.operation {
            validate_operation(operation, &case.references, &catalog, &mut errors);
        }
    }
    errors.sort();
    errors.dedup();
    if !errors.is_empty() {
        return Err(StructuralCatalogErrors(errors));
    }
    let canonical = serde_json::to_vec(&catalog).expect("structural catalog serializes");
    Ok((
        catalog.catalog_id,
        format!("{:x}", Sha256::digest(canonical)),
    ))
}

fn validate_operation(
    operation: &Operation,
    references: &[PageRef],
    catalog: &StructuralCatalog,
    errors: &mut Vec<&'static str>,
) {
    match operation {
        Operation::Merge {
            inputs,
            observation,
        } => {
            let expected = inputs.iter().flatten().cloned().collect::<Vec<_>>();
            if expected.as_slice() != references {
                errors.push("case_references_invalid");
            }
            if !same_origins(&expected, &observation.pages) {
                errors.push("merge_sequence_invalid");
            } else {
                validate_observation(&expected, observation, catalog, 0, errors);
            }
        }
        Operation::Split {
            selection,
            partitions,
        } => {
            if selection.as_slice() != references {
                errors.push("case_references_invalid");
            }
            let pages = partitions
                .iter()
                .flat_map(|partition| partition.pages.iter())
                .cloned()
                .collect::<Vec<_>>();
            if has_duplicate_refs(&pages) {
                errors.push("split_overlap");
            } else if !same_ref_set(&pages, selection) {
                errors.push("split_coverage_invalid");
            } else if pages != *selection {
                errors.push("split_order_invalid");
            } else {
                if partitions.iter().enumerate().any(|(index, partition)| {
                    partitions[..index]
                        .iter()
                        .any(|prior| prior.name == partition.name)
                }) {
                    errors.push("duplicate_split_partition_name");
                }
                for partition in partitions {
                    if !canonical_id(&partition.name) {
                        errors.push("split_partition_name_invalid");
                    }
                    validate_observation(
                        &partition.pages,
                        &partition.observation,
                        catalog,
                        0,
                        errors,
                    );
                }
            }
        }
        Operation::Rotate {
            selection,
            rotation_degrees,
            observation,
        } => {
            if selection.as_slice() != references {
                errors.push("case_references_invalid");
            }
            if rotation_degrees % 90 != 0 {
                errors.push("rotation_degrees_invalid");
            }
            validate_observation(selection, observation, catalog, *rotation_degrees, errors);
        }
        Operation::Reorder {
            selection,
            permutation,
            observation,
        } => {
            if selection.as_slice() != references {
                errors.push("case_references_invalid");
            }
            if has_duplicate_refs(permutation) || !same_ref_set(permutation, selection) {
                errors.push("reorder_permutation_invalid");
            }
            validate_observation(permutation, observation, catalog, 0, errors);
        }
        Operation::DeleteSelectedPages {
            selection,
            removals,
            observation,
        } => {
            if selection.as_slice() != references {
                errors.push("case_references_invalid");
            }
            if has_duplicate_refs(removals) || !removals.iter().all(|page| selection.contains(page))
            {
                errors.push("delete_removals_invalid");
            }
            let retained = selection
                .iter()
                .filter(|page| !removals.contains(page))
                .cloned()
                .collect::<Vec<_>>();
            validate_observation(&retained, observation, catalog, 0, errors);
        }
    }
}

fn validate_observation(
    expected: &[PageRef],
    observation: &Observation,
    catalog: &StructuralCatalog,
    rotation_degrees: u16,
    errors: &mut Vec<&'static str>,
) {
    if observation.baseline.is_some() {
        errors.push("observation_baseline_forbidden");
        return;
    }
    if observation.page_count != expected.len() || !same_origins(expected, &observation.pages) {
        errors.push("observation_invalid");
        return;
    }
    for observed in &observation.pages {
        let Some(page) = catalog
            .sources
            .iter()
            .find(|source| source.id == observed.origin.source_id)
            .and_then(|source| {
                source
                    .pages
                    .iter()
                    .find(|page| page.id == observed.origin.page_id)
            })
        else {
            errors.push("dangling_page_ref");
            continue;
        };
        if page.geometry != Some(observed.geometry)
            || observed.effective_rotation_degrees % 90 != 0
            || (u32::from(page.rotation_degrees.unwrap_or(0)) + u32::from(rotation_degrees)) % 360
                != u32::from(observed.effective_rotation_degrees)
        {
            errors.push("observation_invalid");
        }
    }
}

fn same_origins(expected: &[PageRef], observed: &[ObservedPage]) -> bool {
    expected.iter().eq(observed.iter().map(|page| &page.origin))
}

fn has_duplicate_refs(references: &[PageRef]) -> bool {
    (1..references.len()).any(|index| references[..index].contains(&references[index]))
}

fn same_ref_set(left: &[PageRef], right: &[PageRef]) -> bool {
    left.len() == right.len() && left.iter().all(|reference| right.contains(reference))
}

pub(crate) fn validate_structural_catalog_sources(
    bytes: &[u8],
    repository_root: &Path,
) -> Result<(), StructuralCatalogErrors> {
    let catalog: StructuralCatalog = serde_json::from_slice(bytes)
        .map_err(|_| StructuralCatalogErrors(vec!["structural_catalog_schema_invalid"]))?;
    let mut errors = Vec::new();
    let mut paths = Vec::new();
    for source in &catalog.sources {
        let Some(path) = source.path.as_deref() else {
            errors.push("source_path_missing");
            continue;
        };
        paths.push(path);
        match confined_read(repository_root, path, MAX_SOURCE_BYTES) {
            Ok(contents) => match source.sha256.as_deref() {
                Some(expected) if expected == format!("{:x}", Sha256::digest(contents)) => {}
                Some(_) => errors.push("source_digest_mismatch"),
                None => errors.push("source_digest_missing"),
            },
            Err(code) => errors.push(code),
        }
        validate_authoring(
            repository_root,
            &source.authoring_path,
            &source.authoring_sha256,
            &mut errors,
        );
        for (record, missing, unsafe_record) in [
            (
                source.provenance_path.as_deref(),
                "source_provenance_missing",
                "source_provenance_unsafe",
            ),
            (
                source.license_path.as_deref(),
                "source_license_missing",
                "source_license_unsafe",
            ),
            (
                source.distribution_path.as_deref(),
                "source_distribution_missing",
                "source_distribution_unsafe",
            ),
            (
                source.metadata_path.as_deref(),
                "source_metadata_missing",
                "source_metadata_unsafe",
            ),
        ] {
            validate_record(repository_root, record, missing, unsafe_record, &mut errors);
        }
    }
    paths.sort_unstable();
    if paths.windows(2).any(|pair| pair[0] == pair[1]) {
        errors.push("duplicate_source_path");
    }
    errors.sort_unstable();
    errors.dedup();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(StructuralCatalogErrors(errors))
    }
}

pub(crate) fn validate_structural_documentation(
    repository_root: &Path,
    documents: &[&str],
) -> Result<(), StructuralCatalogErrors> {
    let mut errors = Vec::new();
    let mut text = String::new();
    for document in documents {
        match confined_read(repository_root, document, MAX_SOURCE_BYTES) {
            Ok(contents) => match String::from_utf8(contents) {
                Ok(contents) => {
                    for reference in markdown_references(&contents) {
                        let path = Path::new(document)
                            .parent()
                            .unwrap_or_else(|| Path::new(""))
                            .join(reference);
                        match confined_read(
                            repository_root,
                            &path.to_string_lossy(),
                            MAX_SOURCE_BYTES,
                        ) {
                            Ok(_) => {}
                            Err("source_path_unsafe") => {
                                errors.push("documentation_reference_unsafe")
                            }
                            Err("source_path_missing") => {
                                errors.push("documentation_reference_missing")
                            }
                            Err(_) => errors.push("documentation_reference_unsupported"),
                        }
                    }
                    text.push('\n');
                    text.push_str(&contents.to_ascii_lowercase());
                }
                Err(_) => errors.push("documentation_reference_unsupported"),
            },
            Err("source_path_unsafe") => errors.push("documentation_reference_unsafe"),
            Err("source_path_missing") => errors.push("documentation_reference_missing"),
            Err(_) => errors.push("documentation_reference_unsupported"),
        }
    }
    if [
        "project-authored",
        "clean checkout",
        "structural-only",
        "baseline-free",
    ]
    .iter()
    .any(|required| !text.contains(required))
    {
        errors.push("documentation_truthfulness_missing");
    }
    if documentation_claim_unsupported(&text) {
        errors.push("documentation_claim_unsupported");
    }
    errors.sort_unstable();
    errors.dedup();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(StructuralCatalogErrors(errors))
    }
}

fn documentation_claim_unsupported(text: &str) -> bool {
    text.split(['\n', '.', ';', '!', '?']).any(|clause| {
        let clause = clause.trim();
        let bounded = "no |not |does not|do not|never|without|unavailable|unsupported|deferred|future|conditional|requires|independent verification"
            .split('|')
            .any(|boundary| clause.contains(boundary));
        let known = "engine support|adapter support|comparator behavior|output fidelity|pdf byte equality|visual equality|text equality|content erasure|candidate baseline availability|phase 1 complete"
            .split('|')
            .any(|claim| clause.contains(claim));
        let affirmative = ["support", "supports", "supported", "available", "implemented", "complete"]
            .iter()
            .any(|word| standalone_token(clause, word));
        let target = ["public capability", "redaction"]
            .iter()
            .any(|target| clause.contains(target));
        let explicit_absence = target && standalone_token(clause, "absent") && !affirmative;
        !(bounded || explicit_absence) && (known || affirmative && target)
    })
}

fn standalone_token(clause: &str, token: &str) -> bool {
    clause
        .split(|character: char| !character.is_ascii_alphanumeric())
        .any(|word| word == token)
}

fn markdown_references(document: &str) -> impl Iterator<Item = &str> {
    document
        .split("](")
        .skip(1)
        .filter_map(|suffix| suffix.split_once(')').map(|(reference, _)| reference))
        .filter_map(|reference| {
            reference
                .split_once('#')
                .map_or(Some(reference), |(path, _)| Some(path))
        })
        .filter(|reference| !reference.is_empty())
}

fn validate_record(
    root: &Path,
    path: Option<&str>,
    missing: &'static str,
    unsafe_record: &'static str,
    errors: &mut Vec<&'static str>,
) {
    match path.and_then(|value| confined_read(root, value, MAX_SOURCE_BYTES).ok()) {
        Some(contents) if !contents.is_empty() && std::str::from_utf8(&contents).is_ok() => {}
        _ if path.is_none() => errors.push(missing),
        _ => errors.push(unsafe_record),
    }
}

fn validate_authoring(root: &Path, path: &str, digest: &str, errors: &mut Vec<&'static str>) {
    let digest_valid = digest.len() == 64
        && digest.bytes().all(|byte| {
            byte.is_ascii_digit() || (byte.is_ascii_lowercase() && byte.is_ascii_hexdigit())
        });
    if !digest_valid {
        errors.push("source_authoring_digest_invalid");
    }
    match confined_read(root, path, MAX_SOURCE_BYTES) {
        Ok(contents) if digest_valid => {
            if digest != format!("{:x}", Sha256::digest(contents)) {
                errors.push("source_authoring_digest_mismatch");
            }
        }
        Ok(_) => {}
        Err("source_path_unsafe") => errors.push("source_authoring_path_unsafe"),
        Err("source_path_missing") => errors.push("source_authoring_path_missing"),
        Err("source_path_nonregular") => errors.push("source_authoring_path_nonregular"),
        Err("source_path_oversized") => errors.push("source_authoring_path_oversized"),
        Err(_) => errors.push("source_authoring_path_unsafe"),
    }
}

fn confined_read(root: &Path, value: &str, maximum_size: u64) -> Result<Vec<u8>, &'static str> {
    if value.is_empty()
        || !Path::new(value)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err("source_path_unsafe");
    }
    let candidate = root.join(value);
    let metadata = fs::symlink_metadata(&candidate).map_err(|_| "source_path_missing")?;
    if !metadata.file_type().is_file() {
        return Err("source_path_nonregular");
    }
    if metadata.len() > maximum_size {
        return Err("source_path_oversized");
    }
    let canonical_root = root.canonicalize().map_err(|_| "source_path_unsafe")?;
    let canonical_path = candidate
        .canonicalize()
        .map_err(|_| "source_path_missing")?;
    if !canonical_path.starts_with(canonical_root) {
        return Err("source_path_unsafe");
    }
    fs::read(canonical_path).map_err(|_| "source_path_missing")
}

fn canonical_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn duplicate_ids<T>(
    items: &[T],
    id: impl Fn(&T) -> &String,
    code: &'static str,
    errors: &mut Vec<&'static str>,
) {
    for pair in items.windows(2) {
        if id(&pair[0]) == id(&pair[1]) {
            errors.push(code);
        }
    }
}
