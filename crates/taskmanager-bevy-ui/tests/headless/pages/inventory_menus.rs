//! test-intest: behavior

//! test-intent: behavior
//!
//! Headless behavior tests for the Startup and Sessions action-menu contexts
//! (`src/pages/startup/menu.rs`, `src/pages/sessions/menu.rs`): the same
//! destructive-verb contract the Services menu proves — freeze, arm the
//! shared gate, typed confirm, honest cancel — with each page's own verbs.

use bevy::input::keyboard::KeyCode;
use taskmanager_application::i18n::t;
use taskmanager_application::{AppAction, AppPage, PlatformEffect};
use taskmanager_core::core::services::{ServiceAction, ServiceStatus};
use taskmanager_core::core::session::{SessionControlAction, SessionItem};
use taskmanager_core::core::startup::{
    StartupEntry, StartupEntryId, StartupEntryLocator, StartupImpact, StartupImpactEvidence,
    StartupImpactUnknownReason, StartupScope, StartupSource,
};

use taskmanager_shell::ShellApp;

use crate::menu_modal::ActionMenuContext;
use crate::pages::sessions::menu::SessionMenuModal;
use crate::pages::startup::menu::StartupMenuModal;

// ---- fixtures -----------------------------------------------------------

fn startup_entry(id: &str, name: &str, enabled: bool) -> StartupEntry {
    StartupEntry {
        id: StartupEntryId::new(id),
        name: name.to_owned(),
        exec: format!("/usr/bin/{name}"),
        enabled,
        source: StartupSource::UserService,
        scope: StartupScope::User,
        control_policy: taskmanager_core::core::startup::StartupControlPolicy::Direct,
        locator: StartupEntryLocator::new(format!("user/{id}")),
        impact: StartupImpact::Low,
        impact_evidence: StartupImpactEvidence::Unknown {
            reason: StartupImpactUnknownReason::NotInstrumented,
        },
    }
}

fn session_item(id: &str, user: &str) -> SessionItem {
    SessionItem {
        id: id.to_owned().into(),
        uid: 1000,
        user: user.to_owned(),
        seat: Some("seat0".to_owned()),
        tty: Some("tty2".to_owned()),
        remote: false,
        timestamp: None,
    }
}

fn shelved_startup(entries: Vec<StartupEntry>) -> ShellApp {
    let mut shell = ShellApp::new();
    shell.apply_platform_batch(taskmanager_application::PlatformEventBatch {
        startup_events: vec![taskmanager_application::CorrelatedStartupEvent {
            request_id: taskmanager_platform_contract::RequestId::MIN,
            capability: taskmanager_platform_contract::CapabilityId::STARTUP,
            provider: None,
            sequence: taskmanager_platform_contract::EventSequence::new(1),
            observed_at_ms: 1,
            event: taskmanager_application::StartupEvent::Snapshot(
                taskmanager_platform_contract::PartialSourceSnapshot {
                    items: entries,
                    sources: Vec::new(),
                },
            ),
        }],
        ..taskmanager_application::PlatformEventBatch::default()
    });
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Startup));
    shell
}

fn shelved_sessions(items: Vec<SessionItem>) -> ShellApp {
    let mut shell = ShellApp::new();
    shell.apply_platform_batch(taskmanager_application::PlatformEventBatch {
        session_events: vec![taskmanager_application::CorrelatedSessionEvent {
            request_id: taskmanager_platform_contract::RequestId::MIN,
            capability: taskmanager_platform_contract::CapabilityId::SESSIONS,
            provider: None,
            sequence: taskmanager_platform_contract::EventSequence::new(1),
            observed_at_ms: 1,
            event: taskmanager_application::SessionEvent::Snapshot(
                taskmanager_platform_contract::PartialSourceSnapshot {
                    items,
                    sources: Vec::new(),
                },
            ),
        }],
        ..taskmanager_application::PlatformEventBatch::default()
    });
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Users));
    shell
}

#[test]
fn the_startup_menu_freezes_enable_and_disable_verbs() {
    let mut shell = shelved_startup(vec![
        startup_entry("ssh-agent", "SSH Agent", true),
        startup_entry("clip-sync", "Clipboard Sync", false),
    ]);
    let entry = shell
        .sorted_startup_entries()
        .first()
        .map(|entry| (**entry).clone())
        .expect("fixture entry");
    let mut modal = StartupMenuModal::default();

    assert!(crate::pages::startup::menu::open_for(
        &mut modal, &shell, &entry.id,
    ));
    let spec = modal
        .session
        .as_ref()
        .expect("the menu is open")
        .frozen
        .spec();
    let labels: Vec<_> = spec.items.iter().map(|item| item.label.clone()).collect();
    assert_eq!(
        labels,
        vec![t("startup.enable"), t("startup.disable")],
        "the startup menu offers exactly the two shared verbs"
    );

    // Commit pick 1 (Disable): the frozen entry arms the shared gate with
    // enabled=false — no platform request yet.
    let _ = modal.drive(&mut shell, KeyCode::ArrowDown);
    let _ = modal.drive(&mut shell, KeyCode::Enter);
    assert!(modal.session.is_none(), "a committed menu closes");
    let pending = shell.pending_startup().expect("the gate armed");
    assert!(!pending.enabled, "the Disable verb froze enabled=false");
    assert_eq!(pending.entry.name, "SSH Agent", "the entry froze");

    // The typed confirm re-emits the frozen startup request.
    let effect = shell.confirm_startup_control();
    assert!(
        matches!(effect, Some(PlatformEffect::StartupControl(_))),
        "confirm re-emits the typed startup request, got {effect:?}"
    );
    assert!(shell.pending_confirmation().is_none(), "gate closed");
}

#[test]
fn the_sessions_menu_freezes_disconnect_and_lock_verbs() {
    let mut shell = shelved_sessions(vec![session_item("c2", "devuser")]);
    let session = shell
        .sorted_sessions()
        .first()
        .map(|session| (**session).clone())
        .expect("fixture session");
    let mut modal = SessionMenuModal::default();

    assert!(crate::pages::sessions::menu::open_for(
        &mut modal,
        &shell,
        &session.id,
    ));
    let spec = modal
        .session
        .as_ref()
        .expect("the menu is open")
        .frozen
        .spec();
    let labels: Vec<_> = spec.items.iter().map(|item| item.label.clone()).collect();
    assert_eq!(
        labels,
        vec![t("users.disconnect"), t("users.lock")],
        "the sessions menu offers exactly the two shared verbs"
    );

    // Commit pick 0 (Disconnect): the frozen session arms the shared gate.
    let _ = modal.drive(&mut shell, KeyCode::Enter);
    assert!(modal.session.is_none(), "a committed menu closes");
    let pending = shell.pending_session().expect("the gate armed");
    assert_eq!(
        pending.action,
        SessionControlAction::Disconnect,
        "the menu's verb froze"
    );
    assert_eq!(pending.session.id.as_str(), "c2", "the session froze");

    // The typed confirm re-emits the frozen session request.
    let effect = shell.confirm_session_control();
    assert!(
        matches!(effect, Some(PlatformEffect::SessionControl(_))),
        "confirm re-emits the typed session request, got {effect:?}"
    );
    assert!(shell.pending_confirmation().is_none(), "gate closed");
}

#[test]
fn menus_close_without_arming_on_escape() {
    let mut shell = shelved_startup(vec![startup_entry("ssh-agent", "SSH Agent", true)]);
    let entry = shell
        .sorted_startup_entries()
        .first()
        .map(|entry| (**entry).clone())
        .expect("fixture entry");
    let mut startup_modal = StartupMenuModal::default();
    assert!(crate::pages::startup::menu::open_for(
        &mut startup_modal,
        &shell,
        &entry.id,
    ));
    let _ = startup_modal.drive(&mut shell, KeyCode::Escape);
    assert!(startup_modal.session.is_none());
    assert!(
        shell.pending_confirmation().is_none() && shell.pending_startup().is_none(),
        "cancel arms nothing"
    );

    // ServiceAction remains the Services vocabulary; the startup verbs are
    // enable/disable only, so the shared service gate stays untouched here.
    let _ = ServiceAction::Start;
    let _ = ServiceStatus::Active;
}
