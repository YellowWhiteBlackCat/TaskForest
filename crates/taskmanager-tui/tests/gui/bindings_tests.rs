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

/// The deliberately-unbound set is explicit — anything else unbound means the
/// help surface stopped advertising a chord the shared router still wires.
#[test]
fn only_the_explicit_terminal_exemptions_are_deliberately_unbound() {
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
    for help in taskmanager_shell::presentation::command_help() {
        let entry = declaration
            .entries
            .iter()
            .find(|entry| entry.command == help.command);
        if is_deliberately_unbound(help.command) {
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
/// bindings plus the TUI-local commands) have no `CommandId` and stay
/// outside the matrix but inside the row count — if either layer changes
/// without the other, this composition breaks.
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
        bound
            + taskmanager_shell::shell_local_bindings().len()
            + crate::command_palette::TUI_LOCAL_COMMANDS.len()
    );
}

// ── Action-menu footer hint vocabulary (TUI-003) ────────────────────────────

/// The six action menus paint their footer chord/label pairs from this
/// module's tables. The rendered pairs are pinned under English so any byte
/// drift in the chord tokens, the catalog words, or the `·` separators
/// fails here before any renderer can change.
#[test]
fn action_menu_vocabulary_pairs_match_the_pinned_footers() {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);

    assert_eq!(
        menu_hint_pairs(&ACTION_MENU_HINTS),
        vec![
            (" ↑↓ ", " move · ".to_owned()),
            ("Enter", " select · ".to_owned()),
            ("Esc", " cancel".to_owned()),
        ],
        "the generic footer is ↑↓ move · Enter select · Esc cancel"
    );
    let startup = menu_hint_pairs(&STARTUP_MENU_HINTS);
    assert_eq!(
        startup.iter().map(|(chord, _)| *chord).collect::<Vec<_>>(),
        vec![" ↑↓ ", " Enter ", " Esc "],
        "the startup menu pins its space-padded chord tokens"
    );
    assert_eq!(
        startup
            .iter()
            .map(|(_, label)| label.as_str())
            .collect::<Vec<_>>(),
        vec![" move · ", " select · ", " cancel"],
        "the startup menu shares the generic catalog labels"
    );
    assert_eq!(
        menu_hint_pairs(&COLUMN_MENU_HINTS),
        vec![(" Enter ", " toggle · Esc close".to_owned())],
        "the column menu pins its single combined toggle/close hint"
    );
}

/// Anti-drift end to end: an open action menu's painted footer is exactly
/// the composed bytes of the shared vocabulary table — the renderer takes
/// its pairs from the table, not from hand-written chord/label literals.
#[test]
fn service_menu_footer_paints_the_shared_vocabulary_bytes() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use taskmanager_application::{AppAction, AppPage};

    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);

    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Services));
    assert!(app.open_service_menu(), "the demo exposes a service menu");

    let expected: String = menu_hint_pairs(&ACTION_MENU_HINTS)
        .into_iter()
        .map(|(chord, label)| format!("{chord}{label}"))
        .collect();
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).expect("test terminal");
    terminal
        .draw(|frame| crate::ui::render(frame, &app, crate::TuiTheme::default()))
        .expect("draw");
    let text = terminal.backend().to_string();
    assert!(
        text.contains(&expected),
        "the painted footer must carry the vocabulary bytes {expected:?}, got:\n{text}"
    );
}

#[test]
fn local_registry_is_shared_by_help_and_palette() {
    let palette = crate::TuiApp::palette_rows();
    let help = crate::ui::help::help_rows();
    for command in crate::command_palette::TUI_LOCAL_COMMANDS {
        assert!(
            help.iter()
                .any(|row| row.shortcut == command.binding.shortcut),
            "local shortcut {:?} must be advertised by help",
            command.binding.shortcut
        );
        assert!(
            palette.iter().any(|row| {
                row.shortcut == command.binding.shortcut
                    && row.label == command.binding.label
                    && row.local_action == command.palette_action
            }),
            "local shortcut {:?} must carry the registry palette action",
            command.binding.shortcut
        );
    }
}

/// The `g` registry row's catalog key is normalized: the shared catalog
/// carries the same English copy as the registry's const label (the copy the
/// help localization fold previously kept as a bare const fallback), so
/// adopting the key cannot drift the English help text.
#[test]
fn gpu_chart_metric_catalog_key_matches_the_registry_const_label() {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let declared = crate::command_palette::TUI_LOCAL_COMMANDS
        .iter()
        .find(|command| command.binding.shortcut == "g")
        .expect("the registry declares the g chord");
    assert_eq!(
        taskmanager_application::i18n::t("help.binding.gpu_chart_metric"),
        declared.binding.label,
        "the catalog copy must stay identical to the registry's const label"
    );
}

// ── Surface-protocol footer hint vocabulary (layer 3 presentation) ─────────

/// Coherence: every surface-hint entry cites a declared protocol arm — its
/// `(scope, chord)` resolves through the `TUI_SURFACE_PROTOCOL` table to its
/// own action. A painted footer can never
/// advertise a chord the protocol does not declare, nor the wrong action for
/// its chord.
#[test]
fn surface_hints_cite_declared_protocol_arms() {
    for hint in crate::command_palette::TUI_SURFACE_HINTS {
        assert_eq!(
            crate::command_palette::surface_protocol_action(hint.scope, hint.chord),
            Some(hint.action),
            "the {:?} hint for {:?} must cite the protocol arm it presents",
            hint.scope,
            hint.chord
        );
    }
}

/// Coverage: every action chord the StatusOverlay and ServiceLogPanel scopes
/// declare is painted exactly once (the overlays' toggle footers and the
/// panel's title control run derive from the protocol, not from a second
/// hand-written list); the settings form deliberately paints no protocol
/// footer — its surface owns that copy.
#[test]
fn surface_hints_cover_every_painted_scope_arm_exactly_once() {
    use crate::command_palette::TuiSurfaceScope;
    for scope in [
        TuiSurfaceScope::StatusOverlay,
        TuiSurfaceScope::ServiceLogPanel,
    ] {
        let declared: Vec<_> = crate::command_palette::TUI_SURFACE_PROTOCOL
            .into_iter()
            .filter(|arm| arm.scope == scope)
            .collect();
        let painted: Vec<_> = crate::command_palette::TUI_SURFACE_HINTS
            .into_iter()
            .filter(|hint| hint.scope == scope)
            .collect();
        assert_eq!(
            painted.len(),
            declared.len(),
            "{scope:?}: every declared action chord must paint exactly one hint"
        );
        for arm in declared {
            assert_eq!(
                painted
                    .iter()
                    .filter(|hint| hint.action == arm.action)
                    .count(),
                1,
                "{scope:?}: chord {:?} must paint exactly once",
                arm.chord
            );
        }
    }
    assert!(
        crate::command_palette::TUI_SURFACE_HINTS
            .iter()
            .all(|hint| hint.scope != TuiSurfaceScope::Settings),
        "the settings form owns its footer copy; the protocol scope stays unpainted"
    );
}

/// The rendered pairs are pinned under English so any byte drift in the chord
/// tokens, the catalog words, or the spacing fails here before any renderer
/// can change: the three overlay footers and the panel's title control run.
#[test]
fn surface_hint_vocabulary_pairs_match_the_pinned_footers() {
    use crate::command_palette::{
        TuiSurfaceAction, TuiSurfaceScope, surface_hint_pairs, surface_hint_run,
    };

    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);

    assert_eq!(
        surface_hint_pairs(
            TuiSurfaceScope::StatusOverlay,
            TuiSurfaceAction::ToggleAbout
        ),
        vec![(" i / Esc ", "  Close".to_owned())],
        "the about footer is `i / Esc  Close`"
    );
    assert_eq!(
        surface_hint_pairs(
            TuiSurfaceScope::StatusOverlay,
            TuiSurfaceAction::ToggleHealth
        ),
        vec![(" h / Esc ", "  Close   ".to_owned())],
        "the health footer keeps its three-space tail before the shell-layer T hint"
    );
    assert_eq!(
        surface_hint_pairs(
            TuiSurfaceScope::StatusOverlay,
            TuiSurfaceAction::ToggleContainers
        ),
        vec![(" c / Esc ", "  Close".to_owned())],
        "the containers footer is `c / Esc  Close`"
    );
    assert_eq!(
        surface_hint_run(TuiSurfaceScope::ServiceLogPanel),
        "f follow · p pause · l level · t time",
        "the panel's control run is the four declared protocol arms in table order"
    );
}

/// Anti-drift end to end: the About overlay's painted footer is exactly the
/// composed bytes of the vocabulary table — the renderer takes its pair from
/// the protocol-derived table, not from a hand-written chord/label literal.
#[test]
fn about_overlay_footer_paints_the_surface_hint_vocabulary_bytes() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use crate::command_palette::{TuiSurfaceAction, TuiSurfaceScope, surface_hint_pairs};

    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);

    let expected: String = surface_hint_pairs(
        TuiSurfaceScope::StatusOverlay,
        TuiSurfaceAction::ToggleAbout,
    )
    .into_iter()
    .map(|(token, label)| format!("{token}{label}"))
    .collect();
    let mut app = crate::demo_app();
    app.toggle_about();
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).expect("test terminal");
    terminal
        .draw(|frame| crate::render(frame, &app, crate::TuiTheme::default()))
        .expect("draw");
    let text = terminal.backend().to_string();
    assert!(
        text.contains(&expected),
        "the painted footer must carry the vocabulary bytes {expected:?}, got:\n{text}"
    );
}
