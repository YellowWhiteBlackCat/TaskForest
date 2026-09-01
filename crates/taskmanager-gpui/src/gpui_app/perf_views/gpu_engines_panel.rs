//! Renderer-local pacing for the per-engine GPU utilization projection.
//!
//! The privileged (polkit/pkexec) Intel PMU helper still produces the rows, but
//! it runs once per request on the runtime's bounded engine-rows lane. This
//! module owns only request pacing; the authorization control is rendered by
//! the central Settings permission center. See ADR-023 +
//! `docs/PERMISSION_MODEL.md` Boundary 2.
//!
//! # Honesty contract (the red line)
//!
//! Per-engine data on this host can ONLY come from the privileged helper: the
//! unprivileged PMU path is `RequiresEscalation` (`perf_event_paranoid=2`) and
//! `drm-engine` fdinfo is absent on the `xe` driver. The central Settings
//! permission center projects every non-ready state honestly; the Performance
//! page paints engine mini-cards only from an accepted payload or typed history,
//! NEVER from a fabricated zero-valued engine bar.
//!
//! # Escalation discipline
//!
//! The escalation is strictly on-demand and user-initiated: the app NEVER
//! auto-triggers the polkit prompt. The entry point is the central Settings
//! permission center. On enable, [`spawn_refresh_loop`] submits one
//! engine-rows request per
//! [`POLL_INTERVAL`] through the `PlatformClient` (a non-blocking channel
//! send); the blocking helper invocation happens on the provider lane thread,
//! and the typed answer returns as a `gpu_engine_rows_events` publication that
//! the shared fold correlates and publishes once. After the first
//! success the polkit `.policy`'s `auth_admin_keep` authorizes the session, so
//! subsequent requests do NOT re-prompt — they just refresh the percentages
//! until the user disables the feature. Leaving the GPU page pauses the local
//! cadence but keeps the accepted session available to the central permission
//! center and to the next GPU-page visit.

use std::time::Duration;

use gpui::{AsyncApp, Context, WeakEntity};
use taskmanager_platform_contract::CapabilityId;

use taskmanager_shell::presentation::gpu_engine_rows::{
    GpuEngineRowsAction, present_gpu_engine_rows,
};

use crate::gpui_app::root::RootView;

/// Interval between background refreshes after the first `Success`. The polkit
/// `.policy` uses `auth_admin_keep`, so once authorized for the session
/// subsequent invocations do NOT re-prompt — this just refreshes the busy
/// percentages. Chosen inside the task's 2–3 s guidance band.
const POLL_INTERVAL: Duration = Duration::from_millis(2500);

// ─────────────────────────────────────────────────────────────────────────────
// RootView state-management glue
// ─────────────────────────────────────────────────────────────────────────────
//
// Mirrors the established split (e.g. `root/services.rs`): the RootView methods
// that drive this feature live next to the panel logic. The application tick
// reconciles pacing before `render_gpu` reads the resulting state.

impl RootView {
    /// User clicked "Enable per-engine GPU": start the renderer-local cadence;
    /// every request transition remains application-owned.
    pub fn enable_gpu_engines(&mut self, gpu_index: usize, cx: &mut Context<RootView>) {
        let device_id = self.gpu_engine_rows_device_id(gpu_index);
        let capability_status = self
            .projection()
            .capability_status(&CapabilityId::TELEMETRY_GPU_ENGINES);
        if !matches!(
            present_gpu_engine_rows(
                self.shell.gpu_engine_rows_state(),
                &device_id,
                capability_status,
            )
            .action(),
            GpuEngineRowsAction::Enable
                | GpuEngineRowsAction::Reauthorize
                | GpuEngineRowsAction::Recheck
        ) {
            return;
        }
        let poll_gen = self.start_gpu_engine_polling(gpu_index);
        let weak = cx.weak_entity();
        spawn_refresh_loop(weak, poll_gen, cx);
        cx.notify();
    }

    /// User clicked "Disable": stop the poll chain and reset to the honest idle
    /// state. The generation bump invalidates any in-flight helper invocation.
    pub fn disable_gpu_engines(&mut self, cx: &mut Context<RootView>) {
        self.stop_gpu_engine_polling(false);
        cx.notify();
    }

    /// Reconcile per-engine GPU pacing from the current page/device selection.
    /// The application tick calls this before rendering; render itself never
    /// advances or closes a request session. Re-entering the GPU page resumes
    /// a session enabled from the central Settings surface.
    pub fn reconcile_gpu_engines_visibility(&mut self, cx: &mut Context<RootView>) {
        use crate::gpui_app::root::TopPage;
        use crate::gpui_app::sidebar::SelectedDevice;
        if let (TopPage::Performance, SelectedDevice::Gpu(index)) = (self.page, self.selected) {
            let device_id = self.gpu_engine_rows_device_id(index);
            self.reconcile_gpu_engine_binding(index, device_id);
            if self.gpu_engine_session_allows_polling()
                && let Some(poll_gen) = self.resume_gpu_engine_polling()
            {
                spawn_refresh_loop(cx.entity().downgrade(), poll_gen, cx);
            }
        } else {
            // Settings is a central authorization surface and can be opened
            // while another page is visible. Pause polling here, but retain
            // the binding and accepted state so an authorization request is
            // not immediately torn down on the next 200 ms tick.
            self.suspend_gpu_engine_polling();
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Frontend-paced refresh chain
// ─────────────────────────────────────────────────────────────────────────────

/// Spawn the per-engine GPU refresh chain on the gpui foreground executor.
///
/// Each iteration submits ONE engine-rows request through the `PlatformClient`
/// (a non-blocking channel send — the BLOCKING pkexec/helper invocation happens
/// on the runtime's bounded engine-rows lane thread), then sleeps
/// [`POLL_INTERVAL`] and loops while the generation is current and the state is
/// live. The typed answer returns as a `gpu_engine_rows_events` publication in
/// a later platform batch; `root/gpu_engine_rows.rs` applies it to this state
/// machine. The generation guard terminates the chain when the user disables,
/// switches device, or leaves the GPU panel — exactly the previous poll-loop
/// lifecycle, minus the UI-side blocking helper call.
///
/// A submission error (typed by the client) maps to a terminal panel state via
/// the same apply path, so an absent lane is honestly visible too.
pub(crate) fn spawn_refresh_loop(
    weak: WeakEntity<RootView>,
    poll_gen: u64,
    cx: &mut Context<RootView>,
) {
    cx.spawn(async move |_this, cx: &mut AsyncApp| {
        loop {
            // Submit one request now. Abort the chain if the generation is
            // stale (user disabled / switched device / left the panel) or the
            // submission resolved to a terminal panel state.
            let keep_going = weak
                .update(cx, |v, _cx| {
                    if !v.gpu_engine_polling_is_current(poll_gen) {
                        return false;
                    }
                    let keep_going = v.submit_gpu_engine_rows_refresh();
                    if !keep_going {
                        v.finish_gpu_engine_polling(poll_gen);
                    }
                    keep_going
                })
                .unwrap_or(false);
            if !keep_going {
                return;
            }
            // Pause before the next refresh. Re-check the generation AND the
            // live state after the sleep so a disable / device-switch / page
            // navigation during the wait terminates the chain cleanly.
            gpui::Timer::after(POLL_INTERVAL).await;
            let still_active = weak
                .update(cx, |v, _cx| {
                    let still_active = v.gpu_engine_polling_is_current(poll_gen)
                        && v.gpu_engine_session_allows_polling();
                    if !still_active {
                        v.finish_gpu_engine_polling(poll_gen);
                    }
                    still_active
                })
                .unwrap_or(false);
            if !still_active {
                return;
            }
        }
    })
    .detach();
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests — pure state-machine logic (no gpui, no real pkexec).
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_perf_views_gpu_engines_panel_tests.rs"]
mod tests;
