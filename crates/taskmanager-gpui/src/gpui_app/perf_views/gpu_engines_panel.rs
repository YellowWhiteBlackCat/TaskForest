//! Per-engine GPU utilization panel projected from the application-owned
//! `telemetry.gpu.engines` request session. The privileged
//! (polkit/pkexec) Intel PMU helper still produces the rows, but it now runs
//! ONCE per request on the runtime's bounded engine-rows lane; this panel owns
//! only request pacing. See ADR-023 +
//! `docs/PERMISSION_MODEL.md` Boundary 2.
//!
//! # Honesty contract (the red line)
//!
//! Per-engine data on this host can ONLY come from the privileged helper: the
//! unprivileged PMU path is `RequiresEscalation` (`perf_event_paranoid=2`) and
//! `drm-engine` fdinfo is absent on the `xe` driver. So the section renders an
//! honest projection — every non-ready variant shows a typed placeholder
//! or the typed failure reason, NEVER a fabricated zero-valued engine bar.
//!
//! # Escalation discipline
//!
//! The escalation is strictly on-demand and user-initiated: the app NEVER
//! auto-triggers the polkit prompt. The only entry is the "Enable per-engine
//! GPU" affordance rendered while the shared session is closed. On
//! enable, [`spawn_refresh_loop`] submits one engine-rows request per
//! [`POLL_INTERVAL`] through the `PlatformClient` (a non-blocking channel
//! send); the blocking helper invocation happens on the provider lane thread,
//! and the typed answer returns as a `gpu_engine_rows_events` publication that
//! the shared fold correlates and publishes once. After the first
//! success the polkit `.policy`'s `auth_admin_keep` authorizes the session, so
//! subsequent requests do NOT re-prompt — they just refresh the percentages
//! until the user disables or leaves the GPU panel.

use std::time::Duration;

use gpui::{
    AnyElement, App, AsyncApp, Context, Div, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, WeakEntity, div, px, relative,
};
use taskmanager_application::{
    CapabilityId, CapabilityStatus, DeviceId, GpuEngineMetric, GpuEngineRowsState,
};
use taskmanager_shell::presentation::gpu_engine_rows::{
    GpuEngineRowsAction, GpuEngineRowsPresentation, present_gpu_engine_rows,
};

use crate::gpui_app::elements;
use crate::gpui_app::root::RootView;
use crate::gpui_app::theme::{Color, Theme, tokens, with_alpha};
use crate::i18n;

/// Interval between background refreshes after the first `Success`. The polkit
/// `.policy` uses `auth_admin_keep`, so once authorized for the session
/// subsequent invocations do NOT re-prompt — this just refreshes the busy
/// percentages. Chosen inside the task's 2–3 s guidance band.
const POLL_INTERVAL: Duration = Duration::from_millis(2500);

pub(crate) fn panel_is_visible(
    state: &GpuEngineRowsState,
    device_id: &DeviceId,
    capability_status: Option<CapabilityStatus>,
) -> bool {
    !matches!(
        present_gpu_engine_rows(state, device_id, capability_status),
        GpuEngineRowsPresentation::Unsupported
    )
}

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
    /// advances or closes a request session.
    pub fn reconcile_gpu_engines_visibility(&mut self) {
        use crate::gpui_app::root::TopPage;
        use crate::gpui_app::sidebar::SelectedDevice;
        if let (TopPage::Performance, SelectedDevice::Gpu(index)) = (self.page, self.selected) {
            let device_id = self.gpu_engine_rows_device_id(index);
            self.reconcile_gpu_engine_binding(index, device_id);
        } else {
            self.stop_gpu_engine_polling(true);
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
// Rendering
// ─────────────────────────────────────────────────────────────────────────────

/// Render the per-engine GPU utilization section as a self-contained card.
///
/// The card is added as a footer below the GPU stats panel. It renders the
/// honest state machine: a placeholder + Enable affordance when `NotEscalated`;
/// the real per-engine bars when `Active`; the typed reason + Retry when a
/// failure outcome landed. NEVER a fabricated bar to "fill" the section.
///
/// `gpu_index` is the device index (for the enable/retry callbacks); `state` is
/// `state` is the immutable shared request lifecycle and sole accepted payload
/// authority. Switching devices closes the shared session before this renderer
/// is called.
pub fn render_gpu_engines_panel(
    theme: &Theme,
    gpu_index: usize,
    state: &GpuEngineRowsState,
    device_id: &DeviceId,
    capability_status: Option<CapabilityStatus>,
    cx: &mut Context<RootView>,
) -> Div {
    let card = div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_8)
        .px(tokens::SPACE_10)
        .py(tokens::SPACE_8)
        .rounded(tokens::small_radius(theme))
        .bg(theme.card_bg);
    // Geometry breakpoint for render tests: the per-engine card root. Absent
    // only when the whole GPU device is out of range (render_gpu returns empty).
    // Unconditional (mirrors `tm-status-bar` in elements.rs): gpui's
    // non-test-support `debug_selector` impl is a zero-cost no-op, so this
    // costs nothing in production while letting `debug_bounds` find the card in
    // the headless render tests.
    let card = card.debug_selector(|| "tm-gpu-engines-card".to_string());

    let heading = div()
        .text_size(tokens::FONT_12)
        .font_weight(tokens::FONT_WEIGHT_BOLD.into())
        .text_color(theme.fg_dim)
        .child(i18n::t("gpu.per_engine_title"));

    let body = match present_gpu_engine_rows(state, device_id, capability_status) {
        GpuEngineRowsPresentation::PermissionRequired => not_escalated_body(theme, gpu_index, cx),
        GpuEngineRowsPresentation::Loading => pending_body(theme, "gpu.engines_authenticating"),
        GpuEngineRowsPresentation::Active(engines) => active_body(theme, engines, cx),
        GpuEngineRowsPresentation::PermissionDenied => failure_body(
            theme,
            "gpu.engines_permission_denied",
            "tm-gpu-engines-denied",
            None,
            "gpu.engines_reauthorize",
            gpu_index,
            cx,
        ),
        GpuEngineRowsPresentation::MissingDependency => failure_body(
            theme,
            "gpu.engines_helper_unavailable",
            "tm-gpu-engines-unavail",
            Some("gpu.engines_install_hint"),
            "gpu.engines_recheck",
            gpu_index,
            cx,
        ),
        GpuEngineRowsPresentation::AuthorizationUnavailable => failure_body(
            theme,
            "gpu.engines_auth_unavailable",
            "tm-gpu-engines-auth-unavail",
            None,
            "gpu.engines_recheck",
            gpu_index,
            cx,
        ),
        GpuEngineRowsPresentation::Unsupported => {
            dim_text(theme, i18n::t("gpu.engines_unsupported"))
        }
        GpuEngineRowsPresentation::Failed => failure_body(
            theme,
            "gpu.engines_failed",
            "tm-gpu-engines-failed",
            None,
            "gpu.engines_recheck",
            gpu_index,
            cx,
        ),
    };

    card.child(heading).child(body)
}

/// NotEscalated body: honest placeholder + the "Enable per-engine GPU"
/// affordance. The ONLY entry to the escalation path — the app never
/// auto-prompts.
fn not_escalated_body(theme: &Theme, gpu_index: usize, cx: &mut Context<RootView>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_8)
        .child(dim_text(theme, i18n::t("gpu.engines_requires_auth")))
        .child(action_button(
            theme,
            i18n::t("gpu.enable_per_engine"),
            theme.accent,
            "tm-gpu-engines-enable",
            cx.listener(move |v, _ev, _win, cx| {
                v.enable_gpu_engines(gpu_index, cx);
            }),
        ))
}

/// Active body: render one labeled bar per real [`GpuEngineMetric`], then a
/// "Disable" affordance. An empty engines vec is the honest "helper ran but
/// reported no engines" case — a typed empty message, NOT a fabricated bar.
fn active_body(theme: &Theme, engines: &[GpuEngineMetric], cx: &mut Context<RootView>) -> Div {
    let mut col = div().flex().flex_col().gap(tokens::SPACE_8);
    if engines.is_empty() {
        col = col.child(dim_text(theme, i18n::t("gpu.engines_none_reported")));
    } else {
        for engine in engines {
            col = col.child(engine_row(theme, engine));
        }
    }
    col.child(action_button(
        theme,
        i18n::t("common.disable"),
        theme.fg_dim,
        "tm-gpu-engines-disable",
        cx.listener(move |v, _ev, _win, cx| {
            v.disable_gpu_engines(cx);
        }),
    ))
}

/// One engine row: name + busy % headline, and a proportional accent bar whose
/// fill width is the real `utilization_pct` (clamped to `[0, 100]`). The value
/// came from the helper's typed SUCCESS payload (via the engine-rows lane), so
/// a 0 % bar here is a MEASURED zero, never a fabricated placeholder.
fn engine_row(theme: &Theme, engine: &GpuEngineMetric) -> Div {
    let pct = engine.utilization_pct.clamp(0.0, 100.0);
    let frac = (pct / 100.0).clamp(0.0, 1.0);
    let row = div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_4)
        .child(
            div()
                .flex()
                .flex_row()
                .items_baseline()
                .gap(tokens::SPACE_8)
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .text_size(tokens::FONT_12)
                        .text_color(theme.fg)
                        .child(engine.name.clone()),
                )
                .child(
                    div()
                        .text_size(tokens::FONT_12)
                        .text_color(theme.fg_dim)
                        .child(format!("{:.0}%", pct.round())),
                ),
        )
        // Bar: track = subtle shade; fill = the per-category GPU accent, width
        // proportional to the real busy percent.
        .child(
            div()
                .w_full()
                .h(px(4.0))
                .flex()
                .flex_row()
                .rounded(tokens::xsmall_radius(theme))
                .bg(with_alpha(theme.fg, 0.10))
                .child(
                    div()
                        .h_full()
                        .w(relative(frac))
                        .rounded(tokens::xsmall_radius(theme))
                        .bg(theme.gpu),
                ),
        );
    // Geometry breakpoint per engine bar — the render-test honesty assertion
    // checks this selector is ABSENT in every non-Active state. Unconditional
    // (see the card-root selector note above: gpui's impl is a no-op off test).
    row.debug_selector(|| "tm-gpu-engines-bar".to_string())
}

/// A typed failure body: the localized headline + the helper's own diagnostic
/// detail + a Retry affordance. The headline key + selector differ per variant
/// so a render test can tell `Denied` from `Failed` from `Unavailable`.
fn failure_body(
    theme: &Theme,
    headline_key: &'static str,
    headline_selector: &'static str,
    detail_key: Option<&'static str>,
    action_key: &'static str,
    gpu_index: usize,
    cx: &mut Context<RootView>,
) -> Div {
    let headline = div()
        .text_size(tokens::FONT_12)
        .text_color(theme.warning)
        .child(i18n::t(headline_key).to_owned());
    // Geometry breakpoint so a render test can distinguish the typed failure
    // variant (Denied vs Failed vs Unavailable) by selector. Unconditional.
    let headline = headline.debug_selector(move || headline_selector.to_string());
    let body = div().flex().flex_col().gap(tokens::SPACE_6).child(headline);
    let body = if let Some(detail_key) = detail_key {
        body.child(dim_text(theme, i18n::t(detail_key)))
    } else {
        body
    };
    body.child(action_button(
        theme,
        i18n::t(action_key),
        theme.accent,
        "tm-gpu-engines-retry",
        cx.listener(move |v, _ev, _win, cx| {
            v.enable_gpu_engines(gpu_index, cx);
        }),
    ))
}

/// A brief pending state line (e.g. "Authenticating…"). Honest non-fabricated
/// placeholder while the helper invocation is in flight.
fn pending_body(theme: &Theme, key: &'static str) -> Div {
    dim_text(theme, i18n::t(key))
}

fn dim_text(theme: &Theme, text: &str) -> Div {
    div()
        .text_size(tokens::FONT_12)
        .text_color(theme.fg_dim)
        .child(text.to_owned())
}

/// A keyboard-focusable, clickable text button styled like the disk-view's
/// "SMART health" link (accent label, focus ring, pointer cursor). The
/// `selector` is a test-support geometry breakpoint. Returns [`AnyElement`] so
/// the `Stateful<Div>` produced by `.id(..)` flows uniformly into a `.child(..)`.
fn action_button(
    theme: &Theme,
    label: &str,
    color: Color,
    selector: &'static str,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut App) + 'static,
) -> AnyElement {
    let btn = div()
        .id("gpu-engines-action")
        .focusable()
        .tab_stop(true)
        .focus(elements::focus_ring(theme))
        .cursor_pointer()
        .on_click(on_click)
        .child(
            div()
                .text_size(tokens::FONT_12)
                .text_color(color)
                .child(label.to_owned()),
        );
    // Geometry breakpoint so render tests can locate the Enable / Disable /
    // Retry affordance by id. Unconditional (gpui's impl is a no-op off test).
    let btn = btn.debug_selector(move || selector.to_string());
    btn.into_any_element()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests — pure state-machine logic (no gpui, no real pkexec).
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_perf_views_gpu_engines_panel_tests.rs"]
mod tests;
