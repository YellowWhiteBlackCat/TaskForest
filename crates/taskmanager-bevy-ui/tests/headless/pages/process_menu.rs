//! test-intent: behavior
//!
//! Headless behavior tests for the Applications action menu
//! (`src/pages/processes/menu.rs`): the neutral priority offer (§8.1
//! 语义平价律), the shared control verbs, and the two submission lanes —
//! single-row batch verbs submit through the shell's batch track, the
//! destructive ones arm the shared gate.

use bevy::input::keyboard::KeyCode;
use taskmanager_application::i18n::t;
use taskmanager_application::{AppAction, AppPage, PendingConfirmation};
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process::{
    PriorityTier, ProcessBatchAction, ProcessItem, ProcessScalarObservations,
};
use taskmanager_platform_contract::{
    CapabilityDescriptor, CapabilityId, CapabilitySnapshot, CapabilityStatus,
};
use taskmanager_shell::ShellApp;
use taskmanager_shell::fixture;

use crate::menu_modal::ActionMenuContext;
use crate::pages::processes::menu::{ProcessMenuCtx, ProcessMenuModal, open_for_selected};

// ---- fixtures -----------------------------------------------------------

/// A process whose provider-native start token is available, so the menu can
/// freeze a live identity (`ProcessLiveKey`) and the batch track can freeze a
/// `FrozenProcessIdentity`.
fn token_process(pid: u32, name: &str) -> ProcessItem {
    let mut process = ProcessItem::new(pid, name);
    process.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(u64::from(pid) * 10_000, 1),
        ..Default::default()
    });
    process
}

fn shelved_shell(processes: Vec<ProcessItem>) -> ShellApp {
    let mut shell = ShellApp::new();
    fixture::edit_processes(&mut shell, |shelved| {
        *shelved = Some(processes);
    });
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));
    shell
}

/// The menu with the cursor on the first fixture row (the shell's default).
fn open_menu(shell: &ShellApp) -> ProcessMenuModal {
    let mut modal = ProcessMenuModal::default();
    assert!(
        open_for_selected(&mut modal, shell),
        "the selected row freezes into the menu"
    );
    modal
}

#[test]
fn the_menu_offers_the_shared_control_verbs_and_the_neutral_priority_tiers() {
    let shell = shelved_shell(vec![token_process(100, "alpha")]);
    let spec = open_menu(&shell)
        .session
        .expect("menu is open")
        .frozen
        .spec();

    let labels: Vec<_> = spec.items.iter().map(|item| item.label.clone()).collect();
    assert_eq!(
        labels,
        vec![
            t("proc.end_task"),
            t("proc.end_process_tree"),
            t("proc.suspend"),
            t("proc.resume"),
            t("proc.kill"),
            t("proc.high"),
            t("proc.normal"),
            t("proc.low"),
        ],
        "the menu offers the shared verbs plus the three neutral tiers, in order"
    );
    assert!(spec.items.iter().all(|item| item.enabled));
}

#[test]
fn the_menu_projects_unavailable_process_control_as_disabled() {
    let mut shell = shelved_shell(vec![token_process(100, "alpha")]);
    shell.apply_capability_snapshot(CapabilitySnapshot::from_descriptors([
        CapabilityDescriptor {
            id: CapabilityId::PROCESS_CONTROL,
            status: CapabilityStatus::Unsupported,
            providers: Vec::new(),
            observed_at_ms: 1,
            last_success_at_ms: None,
        },
    ]));
    let spec = open_menu(&shell)
        .session
        .expect("menu is open")
        .frozen
        .spec();

    assert!(
        spec.items.iter().all(|item| !item.enabled),
        "the menu must project capability absence instead of showing clickable actions"
    );
}

#[test]
fn suspend_submits_the_neutral_batch_action_without_arming_the_gate() {
    let mut shell = shelved_shell(vec![token_process(100, "alpha")]);
    let mut modal = open_menu(&shell);

    // Pick 2 is Suspend: one marked row, a non-destructive verb — the batch
    // track submits directly, so the effect is queued for the drain.
    let mut effects = Vec::new();
    let _ = modal.drive(&mut shell, KeyCode::ArrowDown, &mut effects);
    let _ = modal.drive(&mut shell, KeyCode::ArrowDown, &mut effects);
    let _ = modal.drive(&mut shell, KeyCode::Enter, &mut effects);

    assert!(modal.session.is_none(), "a committed menu closes");
    assert_eq!(effects.len(), 1, "exactly one batch effect is queued");
    let taskmanager_application::PlatformEffect::ExecuteBatch(intent) = &effects[0] else {
        panic!(
            "a suspend pick submits the batch track, got {:?}",
            effects[0]
        );
    };
    assert_eq!(intent.action, ProcessBatchAction::Suspend);
    assert_eq!(intent.targets.len(), 1, "the single selected row froze");
    assert!(
        shell.pending_confirmation().is_none(),
        "a non-destructive single-row verb needs no confirmation"
    );
}

#[test]
fn kill_arms_the_shared_batch_gate_and_queues_nothing() {
    let mut shell = shelved_shell(vec![token_process(100, "alpha")]);
    let mut modal = open_menu(&shell);

    for _ in 0..4 {
        let _ = modal.drive(&mut shell, KeyCode::ArrowDown, &mut Vec::new());
    }
    let mut effects = Vec::new();
    let _ = modal.drive(&mut shell, KeyCode::Enter, &mut effects);

    assert!(modal.session.is_none(), "a committed menu closes");
    assert!(
        effects.is_empty(),
        "a destructive verb arms the gate; it never submits from the menu"
    );
    let Some(PendingConfirmation::ProcessBatch(intent)) = shell.pending_confirmation() else {
        panic!("kill arms the shared batch gate");
    };
    assert_eq!(intent.action, ProcessBatchAction::Kill);
}

#[test]
fn the_priority_picks_submit_the_neutral_set_priority_request() {
    let mut shell = shelved_shell(vec![token_process(100, "alpha")]);

    // Pick 5 is the High tier: the neutral action reaches the batch track.
    let mut modal = open_menu(&shell);
    for _ in 0..5 {
        let _ = modal.drive(&mut shell, KeyCode::ArrowDown, &mut Vec::new());
    }
    let mut effects = Vec::new();
    let _ = modal.drive(&mut shell, KeyCode::Enter, &mut effects);
    let taskmanager_application::PlatformEffect::ExecuteBatch(intent) = &effects[0] else {
        panic!(
            "a priority pick submits the batch track, got {:?}",
            effects[0]
        );
    };
    assert_eq!(
        intent.action,
        ProcessBatchAction::SetPriority(PriorityTier::High)
    );

    // Pick 6 is Normal: the same lane, so no tier can silently mean another verb.
    let mut shell = shelved_shell(vec![token_process(200, "beta")]);
    let mut modal = open_menu(&shell);
    for _ in 0..6 {
        let _ = modal.drive(&mut shell, KeyCode::ArrowDown, &mut Vec::new());
    }
    let mut effects = Vec::new();
    let _ = modal.drive(&mut shell, KeyCode::Enter, &mut effects);
    let taskmanager_application::PlatformEffect::ExecuteBatch(intent) = &effects[0] else {
        panic!(
            "a priority pick submits the batch track, got {:?}",
            effects[0]
        );
    };
    assert_eq!(
        intent.action,
        ProcessBatchAction::SetPriority(PriorityTier::Normal)
    );
}

#[test]
fn a_row_without_a_live_identity_keeps_the_menu_closed() {
    // No provider-native start token → no frozen identity → fail closed.
    let shell = shelved_shell(vec![ProcessItem::new(100, "opaque")]);
    let mut modal = ProcessMenuModal::default();
    assert!(
        !open_for_selected(&mut modal, &shell),
        "an unidentifiable row never arms a control verb"
    );
    assert!(modal.session.is_none());
}

#[test]
fn an_empty_table_consumes_no_open_attempt() {
    let shell = shelved_shell(Vec::new());
    let mut modal = ProcessMenuModal::default();
    assert!(!open_for_selected(&mut modal, &shell));
    assert!(modal.session.is_none());
}

#[test]
fn the_frozen_identity_travels_with_the_session() {
    let shell = shelved_shell(vec![
        token_process(100, "alpha"),
        token_process(200, "beta"),
    ]);
    let modal = open_menu(&shell);
    let frozen = modal.session.as_ref().expect("menu is open").frozen;
    assert!(
        matches!(
            frozen,
            ProcessMenuCtx { identity, .. } if identity.stable_key().contains("100")
        ),
        "the menu freezes the row it was opened on, got {frozen:?}"
    );
}
