//! Capability-declaration gate for the Iced shape (CORE-08).

use super::*;
use taskmanager_ui_contract::{capability_drift, capability_findings, capability_report};

/// Contract gate: every capability has exactly one explicit decision and
/// every deliberate difference carries its reason.
#[test]
fn declaration_is_total_and_every_difference_is_registered() {
    let declaration = capability_declaration();
    assert_eq!(
        declaration.frontend,
        taskmanager_ui_contract::FrontendShape::Iced
    );
    assert_eq!(
        declaration.entries.len(),
        taskmanager_ui_contract::ComponentCapability::ALL.len()
    );
    let report = capability_report(&declaration);
    assert!(
        capability_drift(&report).is_empty(),
        "a silent capability omission is drift: {report:?}"
    );
    let findings = capability_findings(&declaration);
    assert!(findings.is_empty(), "{findings:?}");
}

/// The registered divergences are exactly the known architecture drivers —
/// this pins the set so a new divergence cannot appear silently. Text
/// selection left the list when `SelectableText` ported the reference
/// semantics (ICED-008); the footer feedback line remains the one deliberate
/// toast divergence.
#[test]
fn the_registered_divergences_are_exactly_the_known_drivers() {
    let declaration = capability_declaration();
    let divergent: Vec<&str> = declaration
        .entries
        .iter()
        .filter(|entry| {
            matches!(
                entry.support,
                taskmanager_ui_contract::CapabilitySupport::Divergent { .. }
            )
        })
        .map(|entry| entry.capability.id())
        .collect();
    assert_eq!(divergent, ["toast"]);
}
