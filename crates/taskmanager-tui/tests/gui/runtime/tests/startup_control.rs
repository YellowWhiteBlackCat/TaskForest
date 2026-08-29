//! Startup-page Enable/Disable menu, the shared gated confirmation, and the
//! full control round-trip through the shell's pending_startup slot.

use super::super::*;

use taskmanager_application::AppAction;

#[test]
fn enter_on_startup_opens_the_enable_disable_menu_and_esc_closes_it() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Startup));
    assert!(app.startup_menu().is_none());
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(
        app.startup_menu().is_some(),
        "Enter must open the startup menu"
    );
    let entry = app
        .projection()
        .startup_entries
        .as_ref()
        .and_then(|entries| entries.first())
        .expect("demo startup entries");
    assert_eq!(
        app.startup_menu().map(|menu| menu.entry.name.as_str()),
        Some(entry.name.as_str()),
        "the menu freezes the selected row"
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Esc, KeyModifiers::NONE),
    );
    assert!(app.startup_menu().is_none());
}

#[test]
fn startup_menu_select_gates_the_shell_pending_slot_and_y_confirms() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Startup));
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    // The menu's first action is Enable; the fixture entry is already
    // enabled, so pick Disable (Down, Enter) — the gated intent flips state.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Down, KeyModifiers::NONE),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(
        app.shell.pending_startup().is_some(),
        "picking an action must gate the shell's pending_startup slot"
    );
    assert!(app.startup_menu().is_none(), "the menu closes on pick");
    assert_eq!(
        app.shell.pending_startup().map(|pending| pending.enabled),
        Some(false),
        "Disable was selected"
    );

    // y confirms: the platform request is produced with the frozen target.
    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('y'),
            KeyModifiers::NONE,
        ),
    );
    let Some(PlatformEffect::StartupControl(request)) = effect else {
        panic!("confirm must produce a StartupControl effect");
    };
    assert!(!request.entry.id.is_empty());
    assert!(!request.enabled);
    assert_eq!(app.shell.pending_startup(), None);
}

#[test]
fn startup_confirmation_n_dismisses_without_a_platform_effect() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Startup));
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(app.shell.pending_startup().is_some());

    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('n'),
            KeyModifiers::NONE,
        ),
    );
    assert!(effect.is_none(), "n must not emit a platform effect");
    assert_eq!(
        app.shell.pending_startup(),
        None,
        "n clears the pending gate"
    );
}

#[test]
fn startup_menu_cancels_when_the_frozen_entry_is_no_longer_selected() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Startup));
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    // Move the shared cursor off the frozen row (a refresh-style shift), then
    // pick an action: the gate must not open against the stale row.
    app.move_selection(1);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.shell.pending_startup(), None);
    assert!(
        app.shell.feedback_text().contains("cancelled"),
        "the status must explain the cancellation"
    );
}

/// The BN-05 boot timeline renders above the Startup table, but it never
/// joins the selection domain: with the waterfall present, arrows still move
/// the table cursor and Enter still opens the Enable/Disable menu for the
/// selected row (the same keyboard contract as before the block existed).
#[test]
fn boot_timeline_present_arrows_keep_moving_the_table_selection() {
    let mut app = crate::demo_app();
    assert!(
        app.shell.projection().startup_boot_evidence.is_some(),
        "the demo frame seeds typed boot evidence"
    );
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Startup));
    assert_eq!(app.selected, 0);

    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Down, KeyModifiers::NONE),
    );
    assert_eq!(
        app.selected, 1,
        "ArrowDown moves the table cursor; timeline rows are not selectable"
    );

    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    let expected = app
        .projection()
        .startup_entries
        .as_ref()
        .and_then(|entries| entries.get(1))
        .expect("demo startup entries")
        .name
        .clone();
    assert_eq!(
        app.startup_menu().map(|menu| menu.entry.name.as_str()),
        Some(expected.as_str()),
        "Enter opens the menu for the row the cursor is actually on"
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Esc, KeyModifiers::NONE),
    );
    assert!(app.startup_menu().is_none());
}

/// A typed failure in the stashed evidence keeps the page's keyboard path
/// intact: the block is silent but the table contract is unchanged.
#[test]
fn boot_timeline_typed_failure_leaves_the_page_keyboard_contract_intact() {
    use taskmanager_core::core::device_state::DeviceState;
    use taskmanager_core::core::startup::StartupEvidenceFailure;
    let mut app = crate::demo_app();
    let healthy = DeviceState::healthy(1);
    let mut evidence = app
        .shell
        .projection()
        .startup_boot_evidence
        .clone()
        .expect("demo seeds boot evidence");
    evidence.critical_chain_failure = Some(StartupEvidenceFailure::PermissionDenied);
    evidence.critical_chain_state = healthy;
    evidence.failed_units_state = healthy;
    evidence.state = healthy;
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::StartupBootEvidence(Some(evidence)),
    );
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Startup));

    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(
        app.startup_menu().is_some(),
        "Enter must still open the startup menu on the table"
    );
}

/// One synthetic startup row: the provider-issued id is derived from the
/// entry locator so the sorted-vs-provider order assertions are unambiguous.
fn sorted_fixture_entry(id: &str, name: &str) -> taskmanager_core::core::startup::StartupEntry {
    taskmanager_core::core::startup::StartupEntry {
        id: id.into(),
        name: name.into(),
        exec: "fixture-exec".into(),
        enabled: true,
        source: taskmanager_core::core::startup::StartupSource::UserService,
        scope: taskmanager_core::core::startup::StartupScope::User,
        control_policy: taskmanager_core::core::startup::StartupControlPolicy::Direct,
        locator: id.into(),
        impact: taskmanager_core::core::startup::StartupImpact::Low,
        impact_evidence: taskmanager_core::core::startup::StartupImpactEvidence::Unknown {
            reason: taskmanager_core::core::startup::StartupImpactUnknownReason::NotInstrumented,
        },
    }
}

/// An active sort reorders the rendered rows; the Startup Enable/Disable menu
/// must resolve the SAME sorted projection the renderer paints, never the
/// provider-order vector (same contract as the Services page).
#[test]
fn menu_targets_the_sorted_startup_row() {
    let mut app = TuiApp::from_shell(ShellApp::new());
    // Provider order [Beta, Alpha]; the Name sort renders [Alpha, Beta].
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::StartupEntries(Some(vec![
            sorted_fixture_entry("user-service:beta.service", "Beta"),
            sorted_fixture_entry("desktop:alpha.desktop", "Alpha"),
        ])),
    );
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Startup));
    // Select Alpha before sorting so the identity anchor keeps it selected
    // when Name sort moves it to the first rendered row.
    app.selected = 1;
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('s'),
            KeyModifiers::NONE,
        ),
    );
    // Fixture guard: the sort must actually differ from the provider order.
    assert_eq!(
        app.projection()
            .startup_entries
            .as_ref()
            .and_then(|entries| entries.first())
            .map(|entry| entry.id.as_str()),
        Some("user-service:beta.service")
    );

    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        app.startup_menu().map(|menu| menu.entry.id.as_str()),
        Some("desktop:alpha.desktop"),
        "the menu must freeze the sorted (rendered) row, not the provider row"
    );
}
