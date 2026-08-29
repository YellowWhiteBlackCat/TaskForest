//! Capability-declaration gate for the TUI shape (CORE-08).

use super::*;
use taskmanager_ui_contract::{
    CapabilitySupport, ComponentCapability, FrontendShape, capability_drift, capability_findings,
    capability_report,
};

/// Contract gate: every capability has exactly one explicit decision and
/// every divergence/absence carries its terminal driver.
#[test]
fn declaration_is_total_and_every_difference_is_registered() {
    let declaration = capability_declaration();
    assert_eq!(declaration.frontend, FrontendShape::Tui);
    assert_eq!(declaration.entries.len(), ComponentCapability::ALL.len());
    let report = capability_report(&declaration);
    assert!(
        capability_drift(&report).is_empty(),
        "a silent capability omission is drift: {report:?}"
    );
    let findings = capability_findings(&declaration);
    assert!(findings.is_empty(), "{findings:?}");
}

/// The terminal-limit set is pinned: one native (emulator-owned selection),
/// the registered divergences, and the two typed absences. A new difference
/// cannot appear silently.
#[test]
fn the_terminal_limit_set_is_pinned() {
    let declaration = capability_declaration();
    let native: Vec<&str> = declaration
        .entries
        .iter()
        .filter(|entry| matches!(entry.support, CapabilitySupport::Native { .. }))
        .map(|entry| entry.capability.id())
        .collect();
    assert_eq!(native, ["text-selection"]);
    let divergent: Vec<&str> = declaration
        .entries
        .iter()
        .filter(|entry| matches!(entry.support, CapabilitySupport::Divergent { .. }))
        .map(|entry| entry.capability.id())
        .collect();
    assert_eq!(divergent, ["toast", "scrollbar", "focus-visible"]);
    let unsupported: Vec<&str> = declaration
        .entries
        .iter()
        .filter(|entry| matches!(entry.support, CapabilitySupport::Unsupported { .. }))
        .map(|entry| entry.capability.id())
        .collect();
    assert_eq!(unsupported, ["tooltip", "slider", "column-drag-resize"]);
}
