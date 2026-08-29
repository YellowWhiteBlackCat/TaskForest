//! CORE-04 GPUI reference-declaration tests.

use super::functional_declaration;
use taskmanager_ui_contract::{
    FrontendShape, ProductIntent, SurfaceDecision, functional_drift, functional_findings,
    functional_report,
};

#[test]
fn declaration_covers_every_intent_as_the_reference() {
    let declaration = functional_declaration();
    assert_eq!(declaration.frontend, FrontendShape::Gpui);
    assert_eq!(declaration.entries.len(), ProductIntent::ALL.len());
    assert!(functional_drift(&functional_report(&declaration)).is_empty());
    assert!(functional_findings(&declaration).is_empty());
    assert!(
        declaration
            .entries
            .iter()
            .all(|entry| matches!(entry.decision, SurfaceDecision::Reference { .. }))
    );
}
