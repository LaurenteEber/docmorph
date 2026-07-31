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
    authoring_path: Option<String>,
    authoring_sha256: Option<String>,
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
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PageRef {
    source_id: String,
    page_id: String,
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
