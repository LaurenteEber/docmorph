use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
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

fn source_catalog_document() -> String {
    r#"{"schema_version":"2.0","catalog_id":"structural-catalog","sources":[{"id":"source","path":"fixtures/source.pdf","sha256":"b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9","provenance_path":"records/provenance.txt","license_path":"records/license.txt","distribution_path":"records/distribution.txt","metadata_path":"records/metadata.json","pages":[{"id":"page"}]}],"cases":[{"id":"case","output":"output","references":[{"source_id":"source","page_id":"page"}]}]}"#.into()
}

#[test]
fn structural_source_provenance() {
    let root = std::env::temp_dir().join(format!("docmorph-source-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("fixtures")).unwrap();
    fs::create_dir_all(root.join("fixtures/directory")).unwrap();
    fs::create_dir_all(root.join("records")).unwrap();
    fs::write(root.join("fixtures/source.pdf"), b"hello world").unwrap();
    fs::write(root.join("fixtures/large.pdf"), vec![0; 1_048_577]).unwrap();
    for path in [
        "provenance.txt",
        "license.txt",
        "distribution.txt",
        "metadata.json",
    ] {
        fs::write(root.join("records").join(path), b"declared").unwrap();
    }
    let document = source_catalog_document();
    assert!(
        structural_catalog::validate_structural_catalog_sources(document.as_bytes(), &root).is_ok()
    );
    for (actual, expected) in [
        (
            document.replace("fixtures/source.pdf", "../source.pdf"),
            "source_path_unsafe",
        ),
        (
            document.replace("fixtures/source.pdf", "fixtures/missing.pdf"),
            "source_path_missing",
        ),
        (
            document.replace("fixtures/source.pdf", "fixtures/directory"),
            "source_path_nonregular",
        ),
        (
            document.replace("fixtures/source.pdf", "fixtures/large.pdf"),
            "source_path_oversized",
        ),
        (
            document.replace(
                "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9",
                "0000000000000000000000000000000000000000000000000000000000000000",
            ),
            "source_digest_mismatch",
        ),
        (
            document.replace("\"provenance_path\":\"records/provenance.txt\",", ""),
            "source_provenance_missing",
        ),
        (
            document.replace("records/license.txt", "../license.txt"),
            "source_license_unsafe",
        ),
        (
            document.replace("records/distribution.txt", "../distribution.txt"),
            "source_distribution_unsafe",
        ),
        (
            document.replace("records/metadata.json", "../metadata.json"),
            "source_metadata_unsafe",
        ),
    ] {
        assert_eq!(
            structural_catalog::validate_structural_catalog_sources(actual.as_bytes(), &root)
                .unwrap_err()
                .codes(),
            vec![expected]
        );
    }
    let mut duplicate: serde_json::Value = serde_json::from_str(&document).unwrap();
    let mut source = duplicate["sources"][0].clone();
    source["id"] = "source-two".into();
    duplicate["sources"].as_array_mut().unwrap().push(source);
    assert_eq!(
        structural_catalog::validate_structural_catalog_sources(
            serde_json::to_string(&duplicate).unwrap().as_bytes(),
            &root,
        )
        .unwrap_err()
        .codes(),
        vec!["duplicate_source_path"]
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn structural_fixture_regeneration() {
    let root = repository_root();
    let manifest = root.join("fixtures/structural-pdf-corpus-manifest.json");
    let bytes = fs::read(&manifest).expect("structural fixture manifest is distributed");
    structural_catalog::validate_structural_catalog_sources(&bytes, &root)
        .expect("structural fixture sources are authorized");
    let catalog: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let sources = catalog["sources"].as_array().unwrap();
    assert_eq!(sources.len(), 2);
    let mut geometries = BTreeSet::new();
    let mut rotations = BTreeSet::new();
    for (source_index, (source, expected)) in sources
        .iter()
        .zip([
            ("letter-source", "page-1", [612, 792], 0),
            ("landscape-source", "page-2", [842, 595], 90),
        ])
        .enumerate()
    {
        assert_eq!(source["id"], expected.0);
        let authoring = root.join(source["authoring_path"].as_str().unwrap());
        let pdf = root.join(source["path"].as_str().unwrap());
        assert_eq!(
            raw_sha256(&authoring),
            source["authoring_sha256"].as_str().unwrap()
        );
        assert_eq!(raw_sha256(&pdf), source["sha256"].as_str().unwrap());
        let generated =
            std::env::temp_dir().join(format!("docmorph-structural-{source_index}.pdf"));
        assert!(
            Command::new("python3")
                .args([authoring.as_os_str(), generated.as_os_str()])
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(fs::read(&generated).unwrap(), fs::read(&pdf).unwrap());
        let pages = source["pages"].as_array().unwrap();
        assert_eq!(pages.len(), 1);
        let page = &pages[0];
        assert_eq!(page["index"], 0);
        assert_eq!(page["id"], expected.1);
        assert_eq!(page["geometry"], serde_json::json!(expected.2));
        assert_eq!(page["rotation_degrees"], serde_json::json!(expected.3));
        geometries.insert(page["geometry"].to_string());
        rotations.insert(page["rotation_degrees"].as_u64().unwrap());
        let _ = fs::remove_file(generated);
    }
    assert_eq!(geometries.len(), 2);
    assert!(rotations.contains(&90));
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

#[test]
fn structural_merge_split() {
    let document = r#"{"schema_version":"2.0","catalog_id":"structural-catalog","sources":[{"id":"letter-source","pages":[{"id":"page-1","geometry":[612,792],"rotation_degrees":0}]},{"id":"landscape-source","pages":[{"id":"page-2","geometry":[842,595],"rotation_degrees":90}]}],"cases":[{"id":"merge","output":"merged","references":[{"source_id":"letter-source","page_id":"page-1"},{"source_id":"landscape-source","page_id":"page-2"}],"operation":{"kind":"merge","inputs":[[{"source_id":"letter-source","page_id":"page-1"}],[{"source_id":"landscape-source","page_id":"page-2"}]],"observation":{"page_count":2,"pages":[{"origin":{"source_id":"letter-source","page_id":"page-1"},"geometry":[612,792],"effective_rotation_degrees":0},{"origin":{"source_id":"landscape-source","page_id":"page-2"},"geometry":[842,595],"effective_rotation_degrees":90}]} }},{"id":"split","output":"split","references":[{"source_id":"letter-source","page_id":"page-1"},{"source_id":"landscape-source","page_id":"page-2"}],"operation":{"kind":"split","selection":[{"source_id":"letter-source","page_id":"page-1"},{"source_id":"landscape-source","page_id":"page-2"}],"partitions":[{"name":"first","pages":[{"source_id":"letter-source","page_id":"page-1"}],"observation":{"page_count":1,"pages":[{"origin":{"source_id":"letter-source","page_id":"page-1"},"geometry":[612,792],"effective_rotation_degrees":0}]}},{"name":"second","pages":[{"source_id":"landscape-source","page_id":"page-2"}],"observation":{"page_count":1,"pages":[{"origin":{"source_id":"landscape-source","page_id":"page-2"},"geometry":[842,595],"effective_rotation_degrees":90}]}}]}}]}"#;
    assert!(structural_catalog::validate_structural_catalog_bytes(document.as_bytes()).is_ok());
    for (actual, expected) in [
        (document.replacen("\"page_count\":2", "\"baseline\":\"candidate.pdf\",\"page_count\":2", 1), "observation_baseline_forbidden"),
        (document.replacen("\"page_count\":1", "\"baseline\":\"candidate.pdf\",\"page_count\":1", 1), "observation_baseline_forbidden"),
        (document.replacen("\"page_count\":2,", "", 1), "structural_catalog_schema_invalid"),
        (document.replacen("\"page_count\":1,", "", 1), "structural_catalog_schema_invalid"),
        (document.replacen("\"origin\":{\"source_id\":\"letter-source\",\"page_id\":\"page-1\"}", "\"origin\":{\"source_id\":\"landscape-source\",\"page_id\":\"page-2\"}", 1), "merge_sequence_invalid"),
        (document.replacen("\"pages\":[{\"source_id\":\"landscape-source\",\"page_id\":\"page-2\"}]", "\"pages\":[{\"source_id\":\"letter-source\",\"page_id\":\"page-1\"}]", 1), "split_overlap"),
        (document.replacen("\"pages\":[{\"source_id\":\"landscape-source\",\"page_id\":\"page-2\"}]", "\"pages\":[]", 1), "split_coverage_invalid"),
        (document.replacen("\"output\":\"split\",\"references\":[{\"source_id\":\"letter-source\",\"page_id\":\"page-1\"},{\"source_id\":\"landscape-source\",\"page_id\":\"page-2\"}]", "\"output\":\"split\",\"references\":[{\"source_id\":\"landscape-source\",\"page_id\":\"page-2\"},{\"source_id\":\"letter-source\",\"page_id\":\"page-1\"}]", 1).replacen("\"selection\":[{\"source_id\":\"letter-source\",\"page_id\":\"page-1\"},{\"source_id\":\"landscape-source\",\"page_id\":\"page-2\"}]", "\"selection\":[{\"source_id\":\"landscape-source\",\"page_id\":\"page-2\"},{\"source_id\":\"letter-source\",\"page_id\":\"page-1\"}]", 1), "split_order_invalid"),
        (document.replace("\"references\":[{\"source_id\":\"letter-source\",\"page_id\":\"page-1\"},{\"source_id\":\"landscape-source\",\"page_id\":\"page-2\"}]", "\"references\":[{\"source_id\":\"landscape-source\",\"page_id\":\"page-2\"},{\"source_id\":\"letter-source\",\"page_id\":\"page-1\"}]"), "case_references_invalid"),
    ] {
        assert_eq!(
            structural_catalog::validate_structural_catalog_bytes(actual.as_bytes())
            .unwrap_err()
            .codes(),
            vec![expected]
        );
    }
}
