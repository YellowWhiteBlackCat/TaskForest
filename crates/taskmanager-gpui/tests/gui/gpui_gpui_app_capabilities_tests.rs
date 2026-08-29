//! Capability-declaration gate for the GPUI shape (CORE-08): totality,
//! reference role, and the real existence of every reference component.

use super::*;
use std::path::Path;
use taskmanager_ui_contract::{capability_drift, capability_findings, capability_report};

/// Contract gate: every capability is declared exactly once with an
/// explicit decision, and this shape owns the reference role everywhere.
#[test]
fn declaration_covers_every_capability_as_the_reference() {
    let declaration = capability_declaration();
    assert_eq!(declaration.frontend, FrontendShape::Gpui);
    assert_eq!(declaration.entries.len(), ComponentCapability::ALL.len());
    assert!(
        capability_drift(&capability_report(&declaration)).is_empty(),
        "a silent capability omission is drift: {:?}",
        capability_report(&declaration)
    );
    assert!(
        capability_findings(&declaration).is_empty(),
        "{:?}",
        capability_findings(&declaration)
    );
    assert!(
        declaration
            .entries
            .iter()
            .all(|entry| entry.support == CapabilitySupport::Reference)
    );
}

/// The reference pairing is real, not aspirational: every capability's
/// `reference_path` exists under `taskmanager-ui/src`. Renaming or removing
/// a reference component breaks this gate instead of silently orphaning
/// the parallel frontends' declarations (GPUI-05).
#[test]
fn every_reference_component_exists_in_taskmanager_ui() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    for capability in ComponentCapability::ALL {
        let reference = manifest_dir
            .join("../taskmanager-ui/src")
            .join(capability.reference_path());
        assert!(
            reference.is_file(),
            "capability `{}` references missing component {} (GPUI-05)",
            capability.id(),
            reference.display()
        );
    }
}
