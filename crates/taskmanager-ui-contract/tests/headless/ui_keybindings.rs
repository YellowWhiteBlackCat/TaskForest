use super::*;

fn declaration(frontend: FrontendShape, entries: Vec<BindingEntry>) -> FrontendBindingDeclaration {
    FrontendBindingDeclaration { frontend, entries }
}

#[test]
fn bound_and_unbound_entries_carry_their_explicit_status() {
    let known = [CommandId::Refresh, CommandId::ToggleSidebar];
    let decl = declaration(
        FrontendShape::Tui,
        vec![
            BindingEntry::bound(CommandId::Refresh, "F5"),
            BindingEntry::unbound(CommandId::ToggleSidebar),
        ],
    );
    let report = coverage_report_over(&decl, &known);
    assert_eq!(
        report,
        vec![
            (CommandId::Refresh, CoverageStatus::Bound("F5")),
            (
                CommandId::ToggleSidebar,
                CoverageStatus::DeliberatelyUnbound
            ),
        ]
    );
}

#[test]
fn a_known_command_absent_from_the_declaration_is_missing() {
    let known = [CommandId::Refresh, CommandId::Dismiss];
    let decl = declaration(
        FrontendShape::Gpui,
        vec![BindingEntry::bound(CommandId::Refresh, "F5")],
    );
    let report = coverage_report_over(&decl, &known);
    assert_eq!(
        report,
        vec![
            (CommandId::Refresh, CoverageStatus::Bound("F5")),
            (CommandId::Dismiss, CoverageStatus::Missing),
        ]
    );
    assert_eq!(
        drift_findings(&report),
        vec![(CommandId::Dismiss, CoverageStatus::Missing)]
    );
}

#[test]
fn declaring_one_command_twice_is_duplicated() {
    let known = [CommandId::Confirm];
    let decl = declaration(
        FrontendShape::Iced,
        vec![
            BindingEntry::bound(CommandId::Confirm, "Enter"),
            BindingEntry::unbound(CommandId::Confirm),
        ],
    );
    let report = coverage_report_over(&decl, &known);
    assert_eq!(
        report,
        vec![(CommandId::Confirm, CoverageStatus::Duplicated)]
    );
}

#[test]
fn a_declared_command_outside_the_known_set_is_reported_unknown() {
    let known = [CommandId::Refresh];
    let decl = declaration(
        FrontendShape::Tui,
        vec![
            BindingEntry::bound(CommandId::Refresh, "F5"),
            BindingEntry::bound(CommandId::EndTask, "Delete"),
        ],
    );
    let report = coverage_report_over(&decl, &known);
    assert_eq!(
        report,
        vec![
            (CommandId::Refresh, CoverageStatus::Bound("F5")),
            (CommandId::EndTask, CoverageStatus::Unknown),
        ]
    );
    assert_eq!(
        drift_findings(&report),
        vec![(CommandId::EndTask, CoverageStatus::Unknown)]
    );
}

#[test]
fn the_contract_report_lists_every_command_in_canonical_order() {
    let decl = declaration(FrontendShape::Gpui, Vec::new());
    let report = coverage_report(&decl);
    let commands: Vec<CommandId> = report.iter().map(|(command, _)| *command).collect();
    assert_eq!(commands, CommandId::ALL.to_vec());
    assert!(
        report
            .iter()
            .all(|(_, status)| *status == CoverageStatus::Missing),
        "an empty declaration must expose every command as missing, not hide it"
    );
}

#[test]
fn a_complete_declaration_produces_no_drift_findings() {
    let entries = CommandId::ALL
        .into_iter()
        .enumerate()
        .map(|(index, command)| {
            if index % 2 == 0 {
                BindingEntry::bound(command, "K")
            } else {
                BindingEntry::unbound(command)
            }
        })
        .collect();
    let decl = declaration(FrontendShape::Tui, entries);
    let report = coverage_report(&decl);
    assert_eq!(report.len(), CommandId::ALL.len());
    assert!(drift_findings(&report).is_empty(), "{report:?}");
}

#[test]
fn binding_accessors_expose_token_and_boundness() {
    assert_eq!(Binding::Key("F9").key_token(), Some("F9"));
    assert!(Binding::Key("F9").is_bound());
    assert_eq!(Binding::Unbound.key_token(), None);
    assert!(!Binding::Unbound.is_bound());
}

#[test]
fn frontend_shape_names_are_distinct_and_stable() {
    let names: Vec<_> = FrontendShape::ALL
        .into_iter()
        .map(FrontendShape::name)
        .collect();
    for name in names {
        assert!(!name.is_empty());
    }
    assert_eq!(
        FrontendShape::ALL.map(FrontendShape::name),
        ["gpui", "iced", "tui", "bevy"]
    );
}
