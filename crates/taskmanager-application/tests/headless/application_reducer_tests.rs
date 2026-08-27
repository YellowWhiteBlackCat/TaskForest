use super::*;
use crate::{RefreshRequest, ServiceAction, ServiceId, SurfaceKind};

fn selected_state() -> AppState {
    AppState {
        selected_process: Some(
            FrozenProcessIdentity::from_authoritative_parts(88, "renderer", 500, 5_000)
                .expect("fixture identity"),
        ),
        ..AppState::default()
    }
}

fn service_target(name: &str, action: ServiceAction) -> ServiceControlTarget {
    ServiceControlTarget {
        service_id: ServiceId::new(name),
        action,
    }
}

#[test]
fn destructive_action_requires_request_then_explicit_confirmation() {
    let mut state = selected_state();
    let request = reduce(&mut state, AppAction::RequestEndTask);
    assert_eq!(request.ui, None);
    assert_eq!(
        request.surface,
        SurfaceTransition::Opened(SurfaceKind::Confirmation(ConfirmationKind::EndTask))
    );
    assert_eq!(request.platform, None);

    let confirm = reduce(&mut state, AppAction::ConfirmEndTask);
    assert!(matches!(
        confirm.platform,
        Some(PlatformEffect::EndTask(ref target))
            if target.pid == 88 && target.authoritative_start_token() == Some(5_000)
    ));
    assert!(!state.interaction.is_open());
}

#[test]
fn service_control_requires_request_then_explicit_confirmation() {
    // Stopping a session-critical service (e.g. NetworkManager) is at least
    // as destructive as ending a process, so the gate mirrors EndTask:
    // request only shows the overlay; the platform request is deferred until
    // explicit confirmation. Cancel/Escape (DismissOverlay) must perform NO
    // submit — the inverted-safety bug this gate exists to prevent.
    let mut state = AppState {
        selected_service_control: Some(service_target("NetworkManager", ServiceAction::Stop)),
        ..AppState::default()
    };

    // 1. Request: opens the confirmation surface, emits NO platform work.
    let request = reduce(&mut state, AppAction::RequestServiceControl);
    assert_eq!(request.ui, None);
    assert_eq!(request.platform, None);
    assert_eq!(
        state.interaction.confirmation_kind(),
        Some(ConfirmationKind::ServiceControl)
    );

    // 2. Cancel / Escape / scrim (DismissOverlay): clears the overlay and
    //    performs NO submit. This is the load-bearing safety assertion.
    let dismissed = reduce(&mut state, AppAction::DismissOverlay);
    assert_eq!(dismissed.platform, None);
    assert_eq!(dismissed.ui, None);
    assert!(!state.interaction.is_open());

    // 3. A stray Confirm with nothing pending is a no-op (defense-in-depth:
    //    a stale confirm after dismiss cannot fire a request).
    let stray = reduce(&mut state, AppAction::ConfirmServiceControl);
    assert_eq!(stray.platform, None);
    assert_eq!(stray.ui, None);

    // 4. Re-request, then explicit confirm: the sole path that emits the
    //    platform effect, carrying the exact frozen target.
    let _ = reduce(&mut state, AppAction::RequestServiceControl);
    let confirm = reduce(&mut state, AppAction::ConfirmServiceControl);
    assert!(matches!(
        confirm.platform,
        Some(PlatformEffect::ServiceControl(ref target))
            if target.service_id.as_str() == "NetworkManager"
                && target.action == ServiceAction::Stop
    ));
    assert!(!state.interaction.is_open());

    // 5. ConfirmEndTask cannot hijack a service-control overlay (and vice
    //    versa): the typed overlay variants are disjoint.
    state.selected_service_control = Some(service_target("dbus", ServiceAction::Disable));
    let _ = reduce(&mut state, AppAction::RequestServiceControl);
    let cross = reduce(&mut state, AppAction::ConfirmEndTask);
    assert_eq!(cross.platform, None);
    assert_eq!(
        state.interaction.confirmation_kind(),
        Some(ConfirmationKind::ServiceControl)
    );
}

#[test]
fn service_control_request_without_selection_is_a_noop() {
    let mut state = AppState::default();
    let request = reduce(&mut state, AppAction::RequestServiceControl);
    assert_eq!(request.ui, None);
    assert_eq!(request.platform, None);
    assert!(!state.interaction.is_open());
}

#[test]
fn selection_pages_focus_and_overlay_transitions_are_pure_ui_effects() {
    let mut state = selected_state();
    assert_eq!(
        reduce(
            &mut state,
            AppAction::MoveSelection(SelectionDirection::PageDown)
        )
        .ui,
        Some(UiEffect::MoveSelection(SelectionDirection::PageDown))
    );
    assert_eq!(
        reduce(&mut state, AppAction::SelectPage(AppPage::Services)).ui,
        Some(UiEffect::PageChanged(AppPage::Services))
    );
    assert_eq!(state.active_page, AppPage::Services);

    let properties = reduce(&mut state, AppAction::OpenProperties);
    assert_eq!(
        properties.surface,
        SurfaceTransition::Opened(SurfaceKind::ProcessProperties)
    );
    assert_eq!(properties.platform, None);
    assert_eq!(reduce(&mut state, AppAction::DismissOverlay).ui, None);
}

#[test]
fn refresh_is_platform_work_but_pause_is_frontend_scheduler_work() {
    let mut state = AppState::default();
    assert_eq!(
        reduce(&mut state, AppAction::Refresh(RefreshRequest::Processes)).platform,
        Some(PlatformEffect::Refresh(RefreshRequest::Processes))
    );
    assert_eq!(
        reduce(&mut state, AppAction::TogglePause),
        Reduction {
            ui: Some(UiEffect::ToggleTelemetryPause),
            platform: None,
            surface: SurfaceTransition::Unchanged,
        }
    );
    assert_eq!(
        reduce(&mut state, AppAction::ToggleSidebar),
        Reduction {
            ui: Some(UiEffect::ToggleSidebar),
            platform: None,
            surface: SurfaceTransition::Unchanged,
        }
    );
}

#[test]
fn legacy_frozen_identity_cannot_emit_read_or_mutation_effects() {
    let legacy: FrozenProcessIdentity =
        serde_json::from_str(r#"{"pid":88,"name":"renderer","start_time_secs":500}"#)
            .expect("schema-v1 identity");
    let mut state = AppState {
        selected_process: Some(legacy.clone()),
        ..AppState::default()
    };

    assert_eq!(
        reduce(&mut state, AppAction::RequestEndTask),
        Reduction::default()
    );
    assert_eq!(
        reduce(&mut state, AppAction::OpenProperties),
        Reduction::default()
    );

    let _ = state.interaction.reduce(InteractionEvent::ArmConfirmation(
        PendingConfirmation::EndTask(legacy),
    ));
    let confirmed = reduce(&mut state, AppAction::ConfirmEndTask);
    assert_eq!(confirmed.platform, None);
    assert_eq!(
        confirmed.surface,
        SurfaceTransition::Confirmed(ConfirmationKind::EndTask)
    );
    assert!(!state.interaction.is_open());
}
