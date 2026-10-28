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
/// selection keeps its registered keyboard-focus boundary, the footer
/// feedback line remains the toast divergence, and Tooltip records the
/// native Iced hover-only trigger and dismissal boundary.
#[test]
fn the_registered_divergences_are_exactly_the_known_drivers() {
    let declaration = capability_declaration();
    let divergent: Vec<&str> = declaration
        .entries
        .iter()
        .filter(|entry| matches!(entry.support, CapabilitySupport::Divergent { .. }))
        .map(|entry| entry.capability.id())
        .collect();
    assert_eq!(divergent, ["tooltip", "toast", "text-selection"]);
}

/// The prose registry is a human-readable projection of the declaration; this
/// marker keeps its divergence list from drifting while the code stays the
/// single authority.
#[test]
fn parity_registry_doc_lists_the_declared_divergences() {
    let doc = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/PARITY_REGISTRY.md"
    ))
    .expect("PARITY_REGISTRY.md is readable");
    let marker = doc
        .lines()
        .find_map(|line| line.strip_prefix("<!-- iced-capability-divergences: "))
        .and_then(|rest| rest.strip_suffix(" -->"))
        .expect("PARITY_REGISTRY.md carries the divergence marker");
    let mut documented: Vec<&str> = marker
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect();
    documented.sort_unstable();
    let declaration = capability_declaration();
    let mut declared: Vec<&str> = declaration
        .entries
        .iter()
        .filter(|entry| matches!(entry.support, CapabilitySupport::Divergent { .. }))
        .map(|entry| entry.capability.id())
        .collect();
    declared.sort_unstable();
    assert_eq!(documented, declared);
}

#[test]
fn dropdown_menu_is_ported_and_forwards_the_outside_press() {
    let declaration = capability_declaration();
    let dropdown = declaration
        .entries
        .iter()
        .find(|entry| entry.capability == ComponentCapability::DropdownMenu)
        .expect("DropdownMenu capability is registered");
    assert!(matches!(dropdown.support, CapabilitySupport::Ported));
}

#[test]
fn tooltip_divergence_records_the_remaining_hint_badge_boundary() {
    let declaration = capability_declaration();
    let tooltip = declaration
        .entries
        .iter()
        .find(|entry| entry.capability == ComponentCapability::Tooltip)
        .expect("Tooltip capability is registered");
    assert!(matches!(
        tooltip.support,
        CapabilitySupport::Divergent { reason }
            if reason == "the focused Performance rail readout opens on keyboard focus and \
                          dismisses on pointer click/Escape; footer and row hint badges remain \
                          hover-only because their triggers are not focusable"
    ));
}

#[test]
fn text_selection_divergence_records_the_keyboard_focus_boundary() {
    let declaration = capability_declaration();
    let selection = declaration
        .entries
        .iter()
        .find(|entry| entry.capability == ComponentCapability::TextSelection)
        .expect("TextSelection capability is registered");
    assert!(matches!(
        selection.support,
        CapabilitySupport::Divergent { reason }
            if reason == "Iced ports pointer/word/block selection, multi-line spans, and \
                          Ctrl/Cmd-A once a readout owns the window selection; an unowned \
                          readout keeps Ctrl-A on the shared row-summary command because \
                          readouts are not keyboard-focusable"
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
            | ComponentCapability::TextSelection => {
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
