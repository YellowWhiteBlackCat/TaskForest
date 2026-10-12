//! Capability-declaration gate for the Iced shape (CORE-08).

use super::*;
use taskmanager_ui_contract::CapabilitySupport;
use taskmanager_ui_contract::ComponentCapability;
use taskmanager_ui_contract::FrontendShape;
use taskmanager_ui_contract::{capability_drift, capability_findings, capability_report};

/// Contract gate: every capability has exactly one explicit decision and
/// every deliberate difference carries its reason.
#[test]
fn declaration_is_total_and_every_difference_is_registered() {
    let declaration = capability_declaration();
    assert_eq!(declaration.frontend, FrontendShape::Iced);
    assert_eq!(declaration.entries.len(), ComponentCapability::ALL.len());
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
/// selection keeps its registered keyboard/block-selection differences, the
/// footer feedback line remains the toast divergence, Tooltip records the
/// native Iced hover-only trigger and dismissal boundary, and DropdownMenu
/// records Iced's captured outside press.
#[test]
fn the_registered_divergences_are_exactly_the_known_drivers() {
    let declaration = capability_declaration();
    let divergent: Vec<&str> = declaration
        .entries
        .iter()
        .filter(|entry| matches!(entry.support, CapabilitySupport::Divergent { .. }))
        .map(|entry| entry.capability.id())
        .collect();
    assert_eq!(
        divergent,
        ["dropdown-menu", "tooltip", "toast", "text-selection"]
    );
}

#[test]
fn dropdown_menu_divergence_records_outside_press_capture() {
    let declaration = capability_declaration();
    let dropdown = declaration
        .entries
        .iter()
        .find(|entry| entry.capability == ComponentCapability::DropdownMenu)
        .expect("DropdownMenu capability is registered");
    assert!(matches!(
        dropdown.support,
        CapabilitySupport::Divergent { reason }
            if reason == "Iced captures the outside dismissal press so the background target does not activate; GPUI forwards that same press to the background target"
    ));
}

#[test]
fn tooltip_divergence_records_the_native_interaction_boundary() {
    let declaration = capability_declaration();
    let tooltip = declaration
        .entries
        .iter()
        .find(|entry| entry.capability == ComponentCapability::Tooltip)
        .expect("Tooltip capability is registered");
    assert!(matches!(
        tooltip.support,
        CapabilitySupport::Divergent { reason }
            if reason == "Iced widget::tooltip opens immediately on pointer hover and closes on \
                          pointer exit; it has no keyboard-focus trigger and ignores \
                          pointer-click and Escape dismissal"
    ));
}

#[test]
fn every_capability_is_classified_correctly() {
    let declaration = capability_declaration();
    for capability in ComponentCapability::ALL {
        let entry = declaration
            .entries
            .iter()
            .find(|e| e.capability == *capability)
            .unwrap_or_else(|| panic!("capability {capability:?} must be registered"));
        match capability {
            ComponentCapability::Scrollbar => {
                assert!(
                    matches!(entry.support, CapabilitySupport::Native { via } if via == "iced scrollable"),
                    "Scrollbar must be Native via iced scrollable"
                );
            }
            ComponentCapability::Tooltip
            | ComponentCapability::Toast
            | ComponentCapability::TextSelection
            | ComponentCapability::DropdownMenu => {
                assert!(
                    matches!(entry.support, CapabilitySupport::Divergent { reason } if !reason.is_empty()),
                    "{capability:?} must be Divergent with a non-empty reason"
                );
            }
            _ => {
                assert!(
                    matches!(entry.support, CapabilitySupport::Ported),
                    "{capability:?} must be Ported"
                );
            }
        }
    }
}
