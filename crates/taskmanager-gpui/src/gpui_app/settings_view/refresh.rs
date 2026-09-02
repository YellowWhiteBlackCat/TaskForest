//! Refresh-interval slider row of the Settings modal
//! (`taskmanager_ui::inputs::slider`).

use std::time::Duration;

use gpui::{AppContext, Context, Div, Entity, ParentElement, Styled, div};

use taskmanager_ui::inputs::slider::{Slider, SliderState};

use crate::gpui_app::root::RootView;
use taskmanager_application::i18n;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

// The persistent `Entity<SliderState>` for the refresh-interval slider.
//
// `render_settings` / `refresh_row` are stateless free fns re-called every render,
// but the `Entity<SliderState>` MUST persist (it owns the thumb position, drag
// state, and current value). Each window owns one such entity on its `RootView`
// (field `RootView::settings_slider`, created lazily on the first Settings
// render via [`init_slider_entity`]) — never a shared process-wide entity, which
// would cross window boundaries and leak drag/thumb/value state between
// windows (the same ownership rule as `RootView::services_table`).

/// Create the per-window `Entity<SliderState>` (range 0.5–5.0 s, keyboard step
/// 0.1 s) seeded at the current interval. Called at most once per window via
/// `RootView::settings_slider`'s `get_or_insert_with`.
///
/// The value → interval application now lives in [`refresh_row`]'s
/// `on_change` closure (the own slider is callback-driven; it has no event
/// subscription like the gc `SliderEvent::Change` path had), which updates
/// `RootView.telemetry_refresh_policy` through the entity handle.
pub(crate) fn init_slider_entity(cur: f32, cx: &mut Context<RootView>) -> Entity<SliderState> {
    cx.new(|cx| {
        let mut state = SliderState::new(0.5, 5.0, cx);
        state.set_step(0.1, cx);
        state.set_value(cur, cx);
        state
    })
}

/// Refresh-interval row: a labeled `taskmanager_ui::inputs::slider::Slider` + a
/// "{value} s" readout, wired to the collector's live sample interval. Range
/// 0.5–5.0 s; default 1.0 s (the collector's spawn interval in
/// [`crate::gpui_app::root::init`]).
///
/// # Persistence
/// The slider's `Entity<SliderState>` is owned per window on
/// `RootView::settings_slider` (created lazily by the Settings render call
/// site via [`init_slider_entity`]) and threaded in as `slider_entity`; we init
/// `set_value(cur)` once and let correlated platform control update the shared
/// read model.
///
/// # Wiring
/// On every change (drag or keyboard), the `on_change` callback applies a
/// validated local interval; interval changes synchronously resume automatic
/// telemetry. The readout reflects the provider-applied value on the next frame:
/// `RootView::render` re-reads the shared telemetry read model and passes it back
/// as `refresh_secs`.
///
/// `refresh_secs` is threaded in from `root.rs` render (where `self` is in scope)
/// rather than read via `ent.read(cx)` here — reading the entity during render would
/// double-borrow it (render already holds `&mut self`) and panic in the entity map.
pub(super) fn refresh_row(
    t: &Theme,
    refresh_secs: f32,
    slider_entity: Entity<SliderState>,
    cx: &mut Context<RootView>,
) -> Div {
    const MIN_S: f32 = 0.5;
    const MAX_S: f32 = 5.0;
    let cur = refresh_secs.clamp(MIN_S, MAX_S);
    let readout = format!("{:.1} s", cur);

    // Apply the new interval under the RootView entity (the own slider is
    // callback-driven; the old gc subscription path is gone).
    let ent = cx.entity();
    let slider = Slider::new(slider_entity, t.palette()).on_change(move |secs, _win, cx| {
        let ms = (secs * 1000.0).round().max(50.0) as u64;
        ent.update(cx, |v, cx| {
            let interval =
                taskmanager_application::TelemetryInterval::clamped(Duration::from_millis(ms));
            v.telemetry_refresh_policy.apply(
                taskmanager_application::TelemetryRefreshPolicyChange::SetInterval(interval),
            );
            if let Some(platform) = &mut v.platform {
                platform.set_telemetry_interval(interval);
            }
            cx.notify();
        });
    });

    div()
        .flex()
        .flex_col()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_8,
        ))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_13))
                        .text_color(taskmanager_ui::theme_binding::hsla(t.fg))
                        .child(i18n::t("settings.refresh_interval")),
                )
                .child(
                    div()
                        .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_13))
                        .text_color(taskmanager_ui::theme_binding::hsla(t.fg_dim))
                        .child(readout),
                ),
        )
        .child(slider)
}
