use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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
    pages: Vec<Page>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Page {
    id: String,
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
