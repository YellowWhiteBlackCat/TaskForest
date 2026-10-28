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
/// the keyboard-native but non-equivalent controls, and the one typed
/// absence. A new difference cannot appear silently.
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
    assert_eq!(
        divergent,
        [
            "toast",
            "text-input",
            "slider",
            "segmented-control",
            "column-drag-resize",
            "scrollbar",
            "focus-visible",
        ]
    );
    let unsupported: Vec<&str> = declaration
        .entries
        .iter()
        .filter(|entry| matches!(entry.support, CapabilitySupport::Unsupported { .. }))
        .map(|entry| entry.capability.id())
        .collect();
    assert_eq!(unsupported, ["tooltip"]);
}

/// Every terminal difference carries its DELIVERED driver, not a generic
/// "unsupported" note. The wording is pinned so a capability change has to
/// edit the declaration and this test together.
#[test]
fn every_terminal_difference_names_its_delivered_driver() {
    let declaration = capability_declaration();
    let expected: [(&str, &str); 8] = [
        (
            "tooltip",
            "the terminal has no hover surface; key hints ride the footer/status line",
        ),
        (
            "toast",
            "transient feedback renders through the shared footer activity line",
        ),
        (
            "text-input",
            "keyboard cursor, mid-string insertion, Backspace/Delete, Home/End, and pointer caret positioning are wired; the inline field has no selection highlight or selection gesture yet",
        ),
        (
            "slider",
            "settings values use bounded discrete keyboard steps; there is no continuous pointer drag or interpolation",
        ),
        (
            "segmented-control",
            "no generic segmented group is rendered; page/resource selection uses navigation and TUI-003 bindings without a segment traversal contract",
        ),
        (
            "column-drag-resize",
            "the column menu's -/+ stepper resizes the selected tail column within the terminal cell bounds; a terminal has no pointer-driven header-edge drag surface",
        ),
        (
            "scrollbar",
            "key-driven bounded row window; the terminal shape has no pointer scroll rail",
        ),
        (
            "focus-visible",
            "the cursor row is the single focus indicator; there is no pointer/keyboard modality split",
        ),
    ];
    for (capability, reason) in expected {
        let entry = declaration
            .entries
            .iter()
            .find(|entry| entry.capability.id() == capability)
            .unwrap_or_else(|| panic!("{capability} is declared"));
        assert!(
            matches!(entry.support, CapabilitySupport::Divergent { reason: declared } if declared == reason)
                || matches!(entry.support, CapabilitySupport::Unsupported { reason: declared } if declared == reason),
            "{capability} must carry its delivered driver: {:?}",
            entry.support
        );
    }
}
