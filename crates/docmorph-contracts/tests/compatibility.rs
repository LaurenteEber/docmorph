use docmorph_contracts::{ContractVersion, decode_operation, validate_contract_version};
use serde_json::Value;

const SUPPORTED: &str = include_str!("../../../fixtures/contracts/supported-operation-v1.json");
const UNSUPPORTED: &str = include_str!("../../../fixtures/contracts/unsupported-major-v2.json");

#[test]
fn supported_fixture_round_trips_semantically() {
    let operation = decode_operation(SUPPORTED).expect("supported fixture must decode");
    let fixture: Value = serde_json::from_str(SUPPORTED).expect("fixture must be valid JSON");
    let encoded = serde_json::to_value(operation).expect("operation must encode");

    assert_eq!(encoded, fixture);
}

#[test]
fn unsupported_major_returns_a_stable_diagnostic_before_execution() {
    let fixture: Value = serde_json::from_str(UNSUPPORTED).expect("fixture must be valid JSON");
    let version: ContractVersion = serde_json::from_value(fixture["contract_version"].clone())
        .expect("fixture must contain a contract version");

    let diagnostic = validate_contract_version(version).expect_err("major 2 must be rejected");
    assert_eq!(diagnostic.code, "unsupported_contract_version");
    assert!(decode_operation(UNSUPPORTED).is_err());
}
