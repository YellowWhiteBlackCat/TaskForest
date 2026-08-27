use super::*;
use taskmanager_application::CommandId;
use taskmanager_ui_contract::{CoverageStatus, coverage_report, drift_findings};

#[test]
fn page_and_command_help_build_valid_rows() {
    let theme = Theme::default();
    let _ = help_overlay(&theme, 1.0);
}

/// Contract gate: the Iced declaration covers every contract command
/// with an explicit entry — no missing, duplicated, or unknown command
/// — and because Iced wires the complete shared router, every entry is
/// bound. This is also what guarantees the overlay's declaration-driven
/// fold above never silently drops a row.
#[test]
fn binding_declaration_binds_every_contract_command() {
    let declaration = binding_declaration();
    assert_eq!(declaration.frontend, FrontendShape::Iced);
    assert_eq!(declaration.entries.len(), CommandId::ALL.len());
    let report = coverage_report(&declaration);
    assert!(drift_findings(&report).is_empty(), "{report:?}");
    for (command, status) in report {
        assert!(
            matches!(status, CoverageStatus::Bound(_)),
            "{command:?}: {status:?} — Iced advertises the complete shared router"
        );
    }
}

/// Anti-drift between the declaration and the overlay: the declaration
/// mirrors the shared command rows the overlay renders — same commands,
/// same order, same tokens. If either layer changes without the other,
/// this alignment breaks.
#[test]
fn binding_declaration_mirrors_the_help_overlay_command_rows() {
    let declaration = binding_declaration();
    let commands = command_help();
    assert_eq!(declaration.entries.len(), commands.len());
    for (entry, help) in declaration.entries.iter().zip(commands) {
        assert_eq!(entry.command, help.command);
        assert_eq!(entry.binding, Binding::Key(help.shortcut));
    }
}
