//! test-intent: behavior
//!
//! Headless behavior tests for the Services action menu
//! (`src/pages/services/menu.rs`): the destructive-verb slice without a
//! window — Enter-equivalent open freezes the row, the frozen row travels
//! into the shared gate on commit, the typed confirm re-emits the typed
//! request, cancel discards without submitting, and a closed menu consumes
//! nothing.

use bevy::input::keyboard::KeyCode;
use taskmanager_application::i18n::t;
use taskmanager_application::{AppAction, AppPage, PlatformEffect};
use taskmanager_core::core::services::{ServiceAction, ServiceItem, ServiceStatus};

use taskmanager_shell::ShellApp;

use super::menu::{ServiceMenuCtx, ServiceMenuModal, open_for};
use crate::menu_modal::ActionMenuContext;
use crate::pages::services::ServiceSelection;

// ---- fixtures -----------------------------------------------------------

fn service_item(id: &str, name: &str, status: ServiceStatus) -> ServiceItem {
    ServiceItem::from_inventory(
        id,
        name,
        status,
        format!("{name} description"),
        "loaded",
        "active",
        "running",
    )
}

fn shelved_shell(items: &[ServiceItem]) -> ShellApp {
    let mut shell = ShellApp::new();
    shell.apply_platform_batch(taskmanager_application::PlatformEventBatch {
        service_events: vec![taskmanager_application::CorrelatedServiceEvent {
            request_id: taskmanager_platform_contract::RequestId::MIN,
            capability: taskmanager_platform_contract::CapabilityId::SERVICES,
            provider: None,
            sequence: taskmanager_platform_contract::EventSequence::new(1),
            observed_at_ms: 1,
            event: taskmanager_application::ServiceEvent::Snapshot(
                taskmanager_platform_contract::PartialSourceSnapshot {
                    items: items.to_vec(),
                    sources: Vec::new(),
                },
            ),
        }],
        ..taskmanager_application::PlatformEventBatch::default()
    });
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Services));
    shell
}

fn selection_of(shell: &ShellApp) -> ServiceSelection {
    ServiceSelection {
        target: shell.sorted_services().first().map(|s| s.id.clone()),
    }
}

/// The input seam's open-attempt: bare Enter opens for the selection; every
/// other key is not an open attempt.
fn open_driven(
    modal: &mut ServiceMenuModal,
    shell: &mut ShellApp,
    key: KeyCode,
    selection: &ServiceSelection,
) -> bool {
    if key == KeyCode::Enter {
        return selection
            .target
            .as_ref()
            .is_some_and(|target| open_for(modal, shell, target));
    }
    false
}

#[test]
fn the_menu_spec_names_the_five_shared_verbs() {
    let shell = shelved_shell(&[service_item(
        "NetworkManager.service",
        "NetworkManager",
        ServiceStatus::Active,
    )]);
    let service = shell
        .sorted_services()
        .first()
        .map(|service| (**service).clone())
        .expect("fixture service");
    let ctx = ServiceMenuCtx(service);
    let spec = ctx.spec();
    assert_eq!(spec.title, t("svc.service_actions"));
    let labels: Vec<_> = spec.items.iter().map(|item| item.label.clone()).collect();
    assert_eq!(
        labels,
        vec![
            t("svc.start"),
            t("svc.stop"),
            t("svc.restart"),
            t("svc.enable"),
            t("svc.disable"),
        ],
        "the menu offers exactly the shared verbs in TUI order"
    );
    assert!(spec.items.iter().all(|item| item.enabled));
}

#[test]
fn enter_opens_the_menu_and_confirm_arms_the_shared_gate() {
    let mut shell = shelved_shell(&[service_item(
        "NetworkManager.service",
        "NetworkManager",
        ServiceStatus::Active,
    )]);
    let selection = selection_of(&shell);
    let mut modal = ServiceMenuModal::default();

    // Bare Enter on the selected row opens the menu; the row is frozen in.
    assert!(
        open_driven(&mut modal, &mut shell, KeyCode::Enter, &selection),
        "Enter opens the action menu"
    );
    assert_eq!(
        modal
            .session
            .as_ref()
            .map(|session| session.frozen.0.name.clone())
            .as_deref(),
        Some("NetworkManager"),
        "the frozen row travels with the session"
    );

    // Down lands on Stop (clamped list, no wrap) and Enter commits: the
    // frozen intent crosses into the shell's shared gate — no platform
    // request yet.
    let mut effects = Vec::new();
    let _ = modal.drive(&mut shell, KeyCode::ArrowDown, &mut effects);
    let _ = modal.drive(&mut shell, KeyCode::Enter, &mut effects);
    assert!(
        effects.is_empty(),
        "an inventory menu only arms the gate; it queues no platform effect"
    );
    assert!(modal.session.is_none(), "a committed menu closes");
    let pending = shell.pending_service_control().expect("the gate armed");
    assert_eq!(pending.action, ServiceAction::Stop, "the menu's verb froze");
    assert!(shell.pending_confirmation().is_some(), "gate armed");

    // The typed confirm re-emits the frozen service request and closes the
    // gate — the same `ConfirmServiceControl` action the shared table binds.
    let effect = shell.apply_action(AppAction::ConfirmServiceControl);
    assert!(
        matches!(effect, Some(PlatformEffect::ServiceControl(_))),
        "confirm re-emits the typed service request, got {effect:?}"
    );
    assert!(shell.pending_confirmation().is_none(), "gate closed");
}

#[test]
fn escape_cancels_the_menu_without_arming_anything() {
    let mut shell = shelved_shell(&[service_item(
        "NetworkManager.service",
        "NetworkManager",
        ServiceStatus::Active,
    )]);
    let selection = selection_of(&shell);
    let mut modal = ServiceMenuModal::default();

    assert!(open_driven(
        &mut modal,
        &mut shell,
        KeyCode::Enter,
        &selection
    ));
    let mut effects = Vec::new();
    let _ = modal.drive(&mut shell, KeyCode::Escape, &mut effects);
    assert!(effects.is_empty(), "cancel queues no platform effect");
    assert!(modal.session.is_none(), "Escape closes the menu");
    assert!(
        shell.pending_confirmation().is_none() && shell.pending_service_control().is_none(),
        "cancel arms nothing"
    );
}

#[test]
fn a_closed_menu_consumes_nothing() {
    let mut shell = shelved_shell(&[service_item(
        "NetworkManager.service",
        "NetworkManager",
        ServiceStatus::Active,
    )]);
    let selection = selection_of(&shell);
    let mut modal = ServiceMenuModal::default();

    for key in [
        KeyCode::ArrowDown,
        KeyCode::ArrowUp,
        KeyCode::Escape,
        KeyCode::KeyA,
    ] {
        assert!(
            !open_driven(&mut modal, &mut shell, key, &selection),
            "a closed menu consumes nothing ({key:?})"
        );
    }
    assert!(modal.session.is_none());
}
