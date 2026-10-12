//! Selected-process effects stay typed until the frame-tail drain submits them.

use bevy::ecs::observer::On;
use bevy::ecs::system::{NonSendMut, ResMut};
use bevy::ui_widgets::Activate;
use taskmanager_application::PlatformEffect;
use taskmanager_shell::ShellApp;

use crate::app::FrontendTrack;
use crate::input::PendingEffects;

pub(super) fn on_refresh_activated(
    _activate: On<Activate>,
    mut track: NonSendMut<FrontendTrack>,
    mut pending: Option<ResMut<PendingEffects>>,
) {
    if let Some(pending) = pending.as_deref_mut() {
        queue_selected_process_insights(&mut track, pending, true);
    }
}

pub(super) fn queue_selected_process_insights(
    track: &mut FrontendTrack,
    pending: &mut PendingEffects,
    force: bool,
) {
    let Some(target) = track.shell.selected_process_identity() else {
        return;
    };
    if !force
        && (pending.0.iter().any(
            |effect| matches!(effect, PlatformEffect::ProcessInsights(queued) if queued == &target),
        ) || track
            .shell
            .projection()
            .process_insights
            .as_ref()
            .is_some_and(|projection| projection.target == target))
    {
        return;
    }
    if let Some(effect) = track.shell.request_process_insights() {
        pending.0.push(effect);
    }
}

pub(super) fn on_network_escalate_activated(
    _activate: On<Activate>,
    pending: Option<ResMut<PendingEffects>>,
) {
    if let Some(mut pending) = pending {
        pending
            .0
            .push(ShellApp::request_process_network_escalation());
    }
}
