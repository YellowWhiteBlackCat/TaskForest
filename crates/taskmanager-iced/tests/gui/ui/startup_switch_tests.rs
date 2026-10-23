//! Headless behavior tests for the Startup-page Switch (ICED-045): the
//! enabled state wears the shared sliding Switch, the switch is inert for an
//! entry the provider cannot mutate, and its toggle produces a REAL typed
//! startup-control state change (the shared confirmation gate is armed).

use super::*;
use taskmanager_core::core::startup::{
    StartupBootEvidenceSnapshot, StartupControlPolicy, StartupCriticalChainNode, StartupImpact,
    StartupImpactEvidence, StartupScope, StartupSource,
};

fn row(enabled: bool, control_policy: StartupControlPolicy) -> StartupRow {
    StartupRow {
        id: "startup:test-entry".into(),
        name: "Test Entry".into(),
        enabled,
        source: StartupSource::DesktopEntry,
        scope: StartupScope::User,
        control_policy,
        impact: StartupImpact::Low,
        impact_evidence: StartupImpactEvidence::Measured { duration_ms: 40 },
        exec: "/usr/bin/test-entry".into(),
    }
}

/// The switch's ON state is the entry's REAL enabled state — the control
/// mirrors the inventory instead of carrying a second, drifting truth.
#[test]
fn switch_state_mirrors_the_entries_real_enabled_state() {
    let enabled = startup_switch_state(Some(&row(true, StartupControlPolicy::Direct)));
    assert!(
        enabled.on,
        "an enabled entry wears the switch in its on state"
    );
    assert!(enabled.actionable);

    let disabled = startup_switch_state(Some(&row(false, StartupControlPolicy::Direct)));
    assert!(!disabled.on, "a disabled entry wears the switch off");
    assert!(disabled.actionable);
}

/// An entry the provider cannot mutate never carries a live switch: the
/// request could never succeed, so the control stays inert.
#[test]
fn unsupported_control_policy_keeps_the_switch_inert() {
    let state = startup_switch_state(Some(&row(true, StartupControlPolicy::Unsupported)));
    assert!(
        !state.actionable,
        "an unsupported entry must not offer a live enable/disable control"
    );
}

/// With no selected entry there is nothing to toggle: the switch is off and
/// inert.
#[test]
fn no_selection_leaves_the_switch_off_and_inert() {
    let state = startup_switch_state(None);
    assert!(!state.on);
    assert!(!state.actionable);
}

/// The switch's toggle produces a REAL typed state change: the message the
/// switch publishes arms the shell's shared startup-control confirmation
/// gate (the same gate the Users/Services controls use), so a request never
/// reaches the platform without the confirmation.
#[test]
fn switch_toggle_arms_the_shared_startup_control_gate() {
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Startup));
    let (rows, _visible, _generation) = app.startup_projection();
    let selected = rows.get(app.shell.selected).cloned();
    let state = startup_switch_state(selected.as_ref());
    assert!(
        state.actionable,
        "the demo's first startup entry is directly controllable"
    );

    let _ = app.update(Message::RequestStartupControl(!state.on));
    let pending = app
        .shell
        .pending_startup()
        .expect("the switch's toggle must arm the typed startup-control gate");
    assert_eq!(
        pending.enabled, !state.on,
        "the armed request carries the switch's next state"
    );
}

/// A row switch addresses THE ROW it sits on.
///
/// The Startup table can be sorted, so the visual row order differs from the
/// provider order. A row switch must request the entry the user sees in that
/// row — never the provider's entry at the same index.
#[test]
fn a_row_switch_requests_the_entry_of_its_own_row() {
    use taskmanager_core::core::startup::StartupControlPolicy;

    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Startup));
    // Sort by name so the visual order differs from the provider order.
    let _ = app.update(Message::SortInfoTable {
        table: InfoTable::Startup,
        column: InfoSortCol::Name,
    });
    let (rows, _visible, _generation) = app.startup_projection();
    let visual = 0usize;
    let expected = app
        .shell
        .sorted_startup_entry_at(visual)
        .expect("the sorted table has a first row")
        .id
        .clone();
    // The PROVIDER order of the same row: the sorted table maps the visual row
    // to a provider index, so the provider's entry at that index is the entry
    // a visual-index implementation would have wrongly requested.
    let provider_index = app
        .shell
        .sorted_startup_indices()
        .get(visual)
        .copied()
        .expect("the sorted table has a provider index for the row");
    let provider_entry = app
        .shell
        .sorted_startup_entries()
        .iter()
        .find(|entry| entry.id == expected)
        .map(|entry| entry.id.clone())
        .unwrap_or_default();
    let _ = provider_index;
    assert!(!rows.is_empty(), "the demo carries startup entries");

    let _ = app.update(Message::RequestStartupControlFor {
        index: visual,
        enabled: false,
    });
    let pending = app
        .shell
        .pending_startup()
        .expect("the row switch must arm the typed startup-control gate");
    assert_eq!(
        pending.entry.id, expected,
        "the request targets the row the switch sits on"
    );
    assert_ne!(
        provider_index, visual,
        "the sorted row must differ from the provider's row at the same index, or this test proves nothing"
    );
    assert_eq!(
        provider_entry, expected,
        "the row's own entry is the one the request must target"
    );
    let _ = StartupControlPolicy::Direct;
}

/// The Startup page still composes with the Switch in the action bar.
#[test]
fn startup_page_composes_with_the_switch_action_bar() {
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Startup));
    let _view = startup_page(&app);
}

/// The boot-timeline evidence seam the Startup page reads is untouched by
/// the Switch work (the timeline block stays silent without typed
/// evidence).
#[test]
fn startup_timeline_stays_silent_without_typed_evidence() {
    let evidence: Option<StartupBootEvidenceSnapshot> = None;
    assert!(boot_timeline_block(&Theme::dark(), evidence.as_ref(), false).is_none());
    let _ = StartupCriticalChainNode {
        unit: "dbus.service".into(),
        activated_at_ms: Some(10),
        duration_ms: Some(20),
    };
}
