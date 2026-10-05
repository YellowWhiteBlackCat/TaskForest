//! PreUpdate event-port drain (charter boundary 4).
//!
//! Every frame, before anything renders, the frontend drains the platform
//! event port with non-blocking `try_recv` batches, folds each batch into the
//! shared shell track, and commits the frame's refresh intents. The seam core
//! (`run_drain_cycle`) is a plain function over the application client and
//! the shell — no window, no bevy — so the drain contract is testable
//! headlessly; the thin `drain_system` adapter declares its resource access
//! and forwards the folded state into the UI through two observer
//! events: `CapabilitySummaryChanged` (the capability inventory line) and
//! `ShellProjectionFolded` (the pages' data-refresh trigger).

use crate::first_run_modal::SetupState;
use crate::window_surface::{
    WindowSurface, WindowSurfaceChanged, WindowSurfaceCommand, WindowSurfaceKind,
    WindowSurfaceState,
};
use std::time::{SystemTime, UNIX_EPOCH};
use taskmanager_application::first_run::{FirstRunCompletion, FirstRunController};

use bevy::ecs::event::Event;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, NonSendMut, Res, ResMut, SystemParam};
use taskmanager_application::{PlatformClient, PlatformEffect, RefreshRequest};
use taskmanager_platform_contract::CapabilitySnapshot;

use taskmanager_shell::ShellApp;

use crate::app::{FrontendTrack, SharedRuntimeHandle};
use crate::window::WindowPalette;
use taskmanager_application::AppAction;
use taskmanager_application::DesktopAppearanceEvent;
use taskmanager_core::core::appearance::DesktopAppearance;
use taskmanager_core::core::tray::TrayEvent;
use taskmanager_shell::QuitReason;
use taskmanager_shell::queue_effect;

/// Upper bound on how many non-empty event batches one frame drains.
///
/// Same shape as the TUI seam's `EVENT_DRAIN_BATCH`: the port is drained in a
/// tight non-blocking loop so a telemetry burst cannot pile up behind
/// per-frame redraws, while the bound guarantees the frame still reaches its
/// render and quit checks under a sustained flood (leftover events drain on
/// the next frame).
pub(crate) const EVENT_DRAIN_BATCH: usize = 16;

/// Observer event carrying the freshly folded capability snapshot summary.
///
/// Emitted only when the shell's cached capability inventory actually
/// changed; the window's observer rewrites the summary line from it, which is
/// how drain data demonstrably reaches the UI in M0.
#[derive(Event)]
pub(crate) struct CapabilitySummaryChanged(pub(crate) String);

/// Observer event: the drain folded one or more non-empty platform batches
/// into the shell this frame. **This is the pages' data-refresh trigger** —
/// page observers re-read the projection ([`crate::app::ShellTrack`]) when it
/// fires, so live tables/curves repaint exactly when new facts landed and
/// never poll. Domain revisions provide the fine-grained change gate.
#[derive(Event)]
pub(crate) struct ShellProjectionFolded;

/// Observer event carrying the shell's status/feedback line after it changed.
/// Control submissions (`queue_effect` reports the honest outcome) and their
/// async completions both surface here — the Bevy counterpart of the TUI
/// status bar.
#[derive(Event)]
pub(crate) struct FeedbackChanged(pub(crate) String);

/// Last feedback text seen by the drain, for change detection.
#[derive(Resource, Default)]
pub(crate) struct FeedbackCache(pub(crate) Option<String>);

/// One frame's drain outcome, consumed by the UI adapter.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct DrainCycle {
    /// Non-empty batches folded into the shell this frame. `0` means the
    /// port was quiet and nothing downstream should redraw.
    pub(crate) folded_batches: usize,
    pub(crate) setup_changed: bool,
    /// The new capability summary line, present only when the shell's cached
    /// capability inventory changed this frame.
    pub(crate) capability_summary: Option<String>,
    /// Whether a scheduled or post-control refresh intent left this frame.
    pub(crate) refresh_submitted: bool,
    /// Latest observed desktop appearance from the platform runtime.
    pub(crate) appearance: Option<DesktopAppearance>,
}

/// One frame's drain: capability fold, bounded batch drain, refresh intents.
///
/// Ordering mirrors the TUI seam: the read-only capability snapshot is folded
/// first (every renderer consumes the same platform authority), then batches
/// drain until the port runs dry or the batch bound is hit — an empty batch
/// is the common idle case and does nothing. The refresh block honors the
/// shell's pause state and ends by submitting any post-control process-list
/// refresh a completion requested, through the same shared `queue_effect`
/// seam every effect uses.
pub(crate) fn run_drain_cycle(
    client: &mut PlatformClient,
    shell: &mut ShellApp,
    now_ms: u64,
    mut setup: Option<&mut FirstRunController>,
) -> DrainCycle {
    shell.advance_feedback_time(std::time::Duration::from_millis(16));
    let snapshot = client.capabilities().snapshot();
    let capabilities_changed = shell.apply_capability_snapshot(snapshot.clone());
    let mut folded_batches = 0;
    let mut setup_changed = false;
    let mut last_appearance = None;
    for _ in 0..EVENT_DRAIN_BATCH {
        match client.try_drain() {
            Ok(batch) => {
                if batch.is_empty() {
                    break;
                }
                for event in &batch.desktop_appearance_events {
                    let DesktopAppearanceEvent::Snapshot(snapshot) = &event.event;
                    last_appearance = Some(snapshot.value);
                }
                if let Some(setup) = setup.as_deref_mut() {
                    let outcome = setup.fold_batch(&batch);
                    setup_changed |= outcome != FirstRunCompletion::Unchanged;
                    if outcome == FirstRunCompletion::Restart {
                        shell.request_quit(QuitReason::Restart);
                    }
                }
                shell.apply_platform_batch(batch);
                folded_batches += 1;
            }
            Err(error) => {
                // One typed notice per port failure; the loop stops so a
                // persistently broken port cannot flood the feedback
                // lifecycle with sixteen copies of the same error.
                shell.report_event_port_error(error);
                break;
            }
        }
    }
    let mut refresh_submitted = false;
    if !shell.paused() {
        client.set_telemetry_interval(shell.telemetry_interval());
        refresh_submitted |= !client.run_scheduled_refresh(now_ms).is_empty();
    }
    if let Some(effect) = shell.take_process_refresh_request() {
        queue_effect(shell, client, effect);
        refresh_submitted = true;
    }
    if let Some(effect) = shell.take_startup_refresh_request() {
        queue_effect(shell, client, effect);
        refresh_submitted = true;
    }
    if let Some(effect) = shell.take_session_refresh_request() {
        queue_effect(shell, client, effect);
        refresh_submitted = true;
    }
    // The open service-log stream's throttled follow: the shell owns the
    // 1 Hz cadence and the cursor dedup; the drain only carries the request
    // across the same queue_effect seam.
    if let Some(effect) = shell.poll_service_log(now_ms) {
        queue_effect(shell, client, effect);
    }
    DrainCycle {
        folded_batches,
        setup_changed,
        capability_summary: capabilities_changed.then(|| capability_summary_line(&snapshot)),
        refresh_submitted,
        appearance: last_appearance,
    }
}

/// Render the capability inventory as the app shell's one summary line.
///
/// Counts only — never a fabricated zero: an inventory with no observations
/// says so explicitly instead of reporting `0 available`.
pub(crate) fn capability_summary_line(snapshot: &CapabilitySnapshot) -> String {
    let mut available = 0;
    let mut permission_required = 0;
    let mut unsupported = 0;
    let mut other_states = 0;
    let total = snapshot.iter().count();
    for descriptor in snapshot.iter() {
        use taskmanager_platform_contract::CapabilityStatus;
        match descriptor.status {
            CapabilityStatus::Available => available += 1,
            CapabilityStatus::PermissionRequired => permission_required += 1,
            // An escalatable capability is still a pending permission decision;
            // the operator summary counts it with the permission gates rather
            // than the non-actionable other states.
            CapabilityStatus::RequiresEscalation => permission_required += 1,
            CapabilityStatus::Unsupported => unsupported += 1,
            CapabilityStatus::Degraded(_)
            | CapabilityStatus::MissingDependency
            | CapabilityStatus::TemporarilyUnavailable
            | CapabilityStatus::Stale => other_states += 1,
        }
    }
    if total == 0 {
        return "platform runtime: no capability observations yet".to_string();
    }
    format!(
        "platform runtime: {total} capabilities — {available} available, \
         {permission_required} permission required, {unsupported} unsupported, \
         {other_states} other states"
    )
}

/// Wall-clock milliseconds for the scheduler submission timestamps; the
/// scheduler compares against observation timestamps, so virtual time is not
/// a substitute. Saturates at `u64::MAX` past the epoch instead of panicking.
pub(crate) fn unix_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(u64::MAX)
}

/// The optional appearance authorities a folded platform appearance event is
/// written through: the operator's theme preferences, the resolved window
/// palette, and the camera clear color.
///
/// All three are absent in the headless composition, so each stays optional
/// and the drain skips the appearance write it cannot complete.
#[derive(SystemParam)]
pub(crate) struct AppearanceTargets<'w> {
    setup: Option<ResMut<'w, SetupState>>,
    surface: Option<Res<'w, WindowSurfaceState>>,
    /// Persisted theme preferences, when the settings page installed them.
    prefs: Option<ResMut<'w, crate::pages::settings::ThemePreferences>>,
    /// The resolved window palette every renderer reads.
    palette: Option<ResMut<'w, WindowPalette>>,
    /// The camera clear color (absent headless).
    clear: Option<ResMut<'w, bevy::camera::ClearColor>>,
}

/// The `PreUpdate` system: one drain cycle against the shared runtime.
///
/// The very first frame also submits the initial full refresh through the
/// shared `queue_effect` seam (the same request GPUI/TUI submit at startup),
/// so the summary line reflects live platform state instead of staying on its
/// cold-start text. Batches folded this frame trigger
/// [`ShellProjectionFolded`] for the page observers.
pub(crate) fn drain_system(
    runtime: Res<SharedRuntimeHandle>,
    mut track: NonSendMut<FrontendTrack>,
    mut pending: ResMut<crate::input::PendingEffects>,
    mut feedback_cache: ResMut<FeedbackCache>,
    mut tray: Option<ResMut<crate::tray::TrayResource>>,
    mut appearance_targets: AppearanceTargets,
    mut commands: Commands,
) {
    if let Some(tray_res) = tray.as_mut() {
        tray_res.sync_pause_checkmark(track.shell.paused());
        for event in tray_res.drain_events() {
            if let TrayEvent::MenuActivated { id } = event
                && let Some(intent) = crate::tray::resolve_tray_action(id)
            {
                match intent {
                    crate::tray::TrayIntent::Show => {}
                    crate::tray::TrayIntent::TogglePause => {
                        let _ = track.shell.apply_action(AppAction::TogglePause);
                        tray_res.sync_pause_checkmark(track.shell.paused());
                    }
                    crate::tray::TrayIntent::Quit => {
                        track.shell.request_quit(QuitReason::Tray);
                    }
                }
            }
        }
    }

    let mut client = runtime.shared.lock_client();
    if !track.initial_refresh_submitted {
        if let Some(setup) = appearance_targets.setup.as_deref_mut() {
            setup.0.observe(Some(&mut client), unix_now_ms());
        }
        queue_effect(
            &mut track.shell,
            &mut client,
            PlatformEffect::ObserveDesktopAppearance,
        );
        queue_effect(
            &mut track.shell,
            &mut client,
            PlatformEffect::Refresh(RefreshRequest::Dashboard),
        );
        track.initial_refresh_submitted = true;
    }
    let cycle = run_drain_cycle(
        &mut client,
        &mut track.shell,
        unix_now_ms(),
        appearance_targets
            .setup
            .as_deref_mut()
            .map(|setup| &mut setup.0),
    );
    // Effects produced by the input systems cross to the platform here, the
    // one place that holds the client lock — the same `queue_effect` seam
    // every frontend effect uses.
    for effect in pending.0.drain(..) {
        if let PlatformEffect::SetupScript(request) = effect {
            if appearance_targets
                .surface
                .as_ref()
                .is_some_and(|surface| matches!(surface.0, Some(WindowSurface::FirstRun)))
                && let Some(setup) = appearance_targets.setup.as_deref_mut()
            {
                setup
                    .0
                    .request(request.action, Some(&mut client), unix_now_ms());
                commands.trigger(WindowSurfaceChanged);
            }
        } else {
            queue_effect(&mut track.shell, &mut client, effect);
        }
    }
    if cycle.setup_changed {
        if appearance_targets
            .setup
            .as_ref()
            .is_some_and(|setup| setup.0.view().info.is_none())
        {
            commands.trigger(WindowSurfaceCommand::Close(WindowSurfaceKind::FirstRun));
        }
        commands.trigger(WindowSurfaceChanged);
    }
    if cycle.folded_batches > 0 {
        commands.trigger(ShellProjectionFolded);
    }
    if let Some(summary) = cycle.capability_summary {
        commands.trigger(CapabilitySummaryChanged(summary));
    }
    if let Some(appearance) = cycle.appearance
        && let Some(mut prefs) = appearance_targets.prefs
        && prefs.observed_appearance != Some(appearance)
    {
        prefs.observed_appearance = Some(appearance);
        if prefs.mode.is_none()
            && let Some(pal) = appearance_targets.palette.as_deref_mut()
        {
            crate::pages::settings::apply_preferences(
                &prefs,
                pal,
                appearance_targets.clear.as_deref_mut(),
            );
        }
    }
    let feedback = track.shell.feedback_text().to_owned();
    if feedback_cache.0.as_deref() != Some(feedback.as_str()) {
        feedback_cache.0 = Some(feedback.clone());
        commands.trigger(FeedbackChanged(feedback));
    }
}

/// Capture/demo has no provider. Explicit native setup actions still fold an
/// honest unavailable result instead of leaving an unprocessed effect queue.
pub(crate) fn drain_demo_effects(
    mut pending: ResMut<crate::input::PendingEffects>,
    mut setup: ResMut<SetupState>,
    surface: Res<WindowSurfaceState>,
    mut commands: Commands,
) {
    for effect in pending.0.drain(..) {
        if let PlatformEffect::SetupScript(request) = effect
            && matches!(surface.0, Some(WindowSurface::FirstRun))
        {
            setup.0.request(request.action, None, unix_now_ms());
            commands.trigger(WindowSurfaceChanged);
        }
    }
}

#[cfg(test)]
#[path = "../tests/headless/drain.rs"]
mod tests;
