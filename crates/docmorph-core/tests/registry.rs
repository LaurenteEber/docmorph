use std::sync::Arc;

use docmorph_contracts::OperationKind;
use docmorph_core::{MockAdapter, Registry};

#[test]
fn mock_declaration_is_deterministic() {
    let registry = Registry::new(vec![Arc::new(MockAdapter::default())]);

    let declarations = registry.declarations();
    assert_eq!(declarations.len(), 1);
    assert_eq!(declarations[0].0.name, "mock");
    assert_eq!(declarations[0].1[0].operation, OperationKind::MockTransform);
    assert!(declarations[0].1[0].available);
}
