use super::*;
use taskmanager_ui_contract::{CoverageStatus, coverage_report, drift_findings};

/// Contract gate: every command has exactly one explicit entry — no
/// missing, duplicated, or unknown command.
#[test]
fn declaration_covers_every_contract_command_exactly_once() {
    let declaration = binding_declaration();
    assert_eq!(declaration.frontend, FrontendShape::Tui);
    assert_eq!(declaration.entries.len(), CommandId::ALL.len());
    let report = coverage_report(&declaration);
    assert_eq!(report.len(), CommandId::ALL.len());
    assert!(drift_findings(&report).is_empty(), "{report:?}");
}

/// The deliberately-unbound set is exactly the two documented terminal
/// exemptions — anything else unbound means the help surface stopped
/// advertising a chord the shared router still wires.
#[test]
fn only_the_two_terminal_exemptions_are_deliberately_unbound() {
    let report = coverage_report(&binding_declaration());
    let unbound: Vec<CommandId> = report
        .into_iter()
        .filter(|(_, status)| *status == CoverageStatus::DeliberatelyUnbound)
        .map(|(command, _)| command)
        .collect();
    assert_eq!(unbound, DELIBERATELY_UNBOUND.to_vec());
}

/// Every wired command carries the very shortcut token the shared help
/// presentation renders — the declaration never invents a token the
/// overlay does not show, and never shows one it does not declare.
#[test]
fn every_wired_command_carries_the_shortcut_the_help_renders() {
    let declaration = binding_declaration();
    for help in crate::command_help() {
        let entry = declaration
            .entries
            .iter()
            .find(|entry| entry.command == help.command);
        if DELIBERATELY_UNBOUND.contains(&help.command) {
            assert_eq!(
                entry.map(|entry| entry.binding),
                Some(Binding::Unbound),
                "{:?} must stay explicitly unbound",
                help.command
            );
        } else {
            assert_eq!(
                entry.map(|entry| entry.binding),
                Some(Binding::Key(help.shortcut)),
                "{:?} must carry the shared shortcut {:?}",
                help.command,
                help.shortcut
            );
        }
    }
}

/// Anti-drift between the declaration and the rendered help overlay:
/// the overlay's shared rows must be exactly the declaration's bound
/// entries. The terminal-local chords (the shell's five terminal-only
/// bindings plus the seventeen TUI-local overlay bindings) have no
/// `CommandId` and stay outside the matrix but inside the row count —
/// if either layer changes without the other, this composition breaks.
#[test]
fn bound_entries_match_the_help_overlay_row_composition() {
    let declaration = binding_declaration();
    let bound = declaration
        .entries
        .iter()
        .filter(|entry| entry.binding.is_bound())
        .count();
    let rows = crate::ui::help::help_rows();
    assert_eq!(
        rows.len(),
        bound + crate::shell_local_bindings().len() + crate::ui::help::TUI_LOCAL_BINDINGS.len()
    );
}
