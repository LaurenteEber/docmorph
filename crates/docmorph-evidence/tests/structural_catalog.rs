use std::{
    fs,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

#[allow(dead_code)]
#[path = "../src/catalog.rs"]
mod catalog;
#[path = "../src/structural_catalog.rs"]
mod structural_catalog;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn raw_sha256(path: impl AsRef<Path>) -> String {
    format!(
        "{:x}",
        Sha256::digest(fs::read(path).expect("retained artifact is readable"))
    )
}

#[test]
fn retained_mock_artifacts_remain_byte_and_semantically_pinned() {
    let root = repository_root();
    for (path, expected) in [
        (
            "fixtures/corpus-manifest.json",
            "da7be361fe10b679b9bd1b007517a2ae867a06ed2a1db943af5ffc7ff835e8df",
        ),
        (
            "fixtures/evidence-success-manifest.json",
            "9fbe817531007bb19b53350de677e36391009de4e9df7ae35f9cf744079a38e2",
        ),
        (
            "fixtures/evidence-policy-failure-manifest.json",
            "295f6eddcad4794988ebf689a25aa35a4e419c1e0df187178dc74d65a0547eb0",
        ),
        (
            "evidence/success/receipt.json",
            "410d5e09453ad37ddd18c360b93d22cb6c52d9451b4936109bde792d19af08d0",
        ),
        (
            "evidence/policy-failure/receipt.json",
            "2adb3c8d060c747d95e2a3bd5116766da57508cc2ed569c89070a537396c7d79",
        ),
    ] {
        assert_eq!(raw_sha256(root.join(path)), expected, "{path}");
    }

    let validated = catalog::validate_catalog_bytes(
        &fs::read(root.join("fixtures/corpus-manifest.json")).unwrap(),
        &root,
    )
    .expect("retained mock catalog remains valid");
    assert_eq!(validated.catalog_id, "docmorph-phase1-synthetic-smoke");
    assert_eq!(
        validated.revision_sha256,
        "9710b2a0100fc07d98a719054927037fc49a5020f9b16bdf4a9e77f2a8a432de"
    );
    assert_eq!(
        validated.execution_revision_sha256,
        "9a1c4145ba39ae9e2abc13ee8d9541d48cfbcbf9aaf84e674a6be27a57357dd0"
    );

    for (id, manifest_hash, semantic_hash) in [
        (
            "success",
            "9fbe817531007bb19b53350de677e36391009de4e9df7ae35f9cf744079a38e2",
            "0bbfed8f5855a38169f3d7f41eaec3ddb21a2a603177f5443537cbebb643ad79",
        ),
        (
            "policy-failure",
            "295f6eddcad4794988ebf689a25aa35a4e419c1e0df187178dc74d65a0547eb0",
            "d00cfaf50baa0c27b7ffd4eaaf153860506fd5399d069a1ccece3e9dcd477f3a",
        ),
    ] {
        let receipt_path = root.join(format!("evidence/{id}/receipt.json"));
        let receipt: serde_json::Value =
            serde_json::from_slice(&fs::read(receipt_path).unwrap()).unwrap();
        assert_eq!(receipt["schema_version"], "1.2", "{id}");
        assert_eq!(receipt["catalog_id"], validated.catalog_id, "{id}");
        assert_eq!(
            receipt["catalog_revision_sha256"], validated.execution_revision_sha256,
            "{id}"
        );
        assert_eq!(receipt["manifest_sha256"], manifest_hash, "{id}");
        assert_eq!(receipt["semantic_sha256"], semantic_hash, "{id}");
        assert_eq!(
            validated.baseline(id).unwrap().semantic_sha256(),
            semantic_hash,
            "{id}"
        );
    }
}

fn structural_catalog_document() -> String {
    r#"{"schema_version":"2.0","catalog_id":"structural-catalog","sources":[{"id":"source","pages":[{"id":"page"}]}],"cases":[{"id":"case","output":"output","references":[{"source_id":"source","page_id":"page"}]}]}"#.into()
}

#[test]
fn structural_envelope_core() {
    let document = structural_catalog_document();
    let first = structural_catalog::validate_structural_catalog_bytes(document.as_bytes()).unwrap();
    let second =
        structural_catalog::validate_structural_catalog_bytes(document.as_bytes()).unwrap();
    assert_eq!(first.0, "structural-catalog");
    assert_eq!(first.1, second.1);

    let source = r#"{"id":"source","pages":[{"id":"page"}]}"#;
    let case =
        r#"{"id":"case","output":"output","references":[{"source_id":"source","page_id":"page"}]}"#;
    for (actual, expected) in [
        (
            document.replace("\"2.0\"", "\"1.0\""),
            vec!["unsupported_schema_version"],
        ),
        (
            document.replace("structural-catalog", "Catalog"),
            vec!["catalog_id_invalid"],
        ),
        (
            document.replace(source, &format!("{source},{source}")),
            vec!["ambiguous_page_ref", "duplicate_source_id"],
        ),
        (
            document.replace(
                "[{\"id\":\"page\"}]",
                "[{\"id\":\"page\"},{\"id\":\"page\"}]",
            ),
            vec!["ambiguous_page_ref", "duplicate_page_id"],
        ),
        (
            document.replace(case, &format!("{case},{case}")),
            vec!["duplicate_case_id", "duplicate_output_id"],
        ),
        (
            document.replace("\"page_id\":\"page\"", "\"page_id\":\"missing\""),
            vec!["dangling_page_ref"],
        ),
    ] {
        assert_eq!(
            structural_catalog::validate_structural_catalog_bytes(actual.as_bytes())
                .unwrap_err()
                .codes(),
            expected
        );
    }
}
