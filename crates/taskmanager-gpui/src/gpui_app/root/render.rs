//! Rendering for the root application shell.

use super::{
    Hover, InputModality, RootView, TopPage, WindowCorner, alert_ui, device_label,
    init_search_entity, keyboard, nav_strip, responsive, static_label, top_bar,
};
use crate::gpui_app::dashboard::SystemSection;
use crate::gpui_app::system_view;
use crate::gpui_app::theme::{tokens, ui_font_with_fallback, window_chrome_state};
use gpui::{
    Animation, AnimationExt, Context, Div, InteractiveElement, IntoElement, MouseMoveEvent,
    ParentElement, Render, Styled, Window, div, ease_in_out, px,
};
use taskmanager_ui::{focus::restore_modal, layout::page_viewport};
mod overlays;
mod pages;
mod surfaces;
mod transients;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum CursorRefreshState {
    #[default]
    Idle,
    Scheduled,
}

/// True when a cursor move must be reflected by the next RootView render.
/// The tooltip reads the window's current pointer position at render time; row
/// hover state is updated independently by the row's `on_hover` boundary.
/// Keeping this policy pure makes the event-rate reduction testable without a
/// compositor.
#[must_use]
fn should_schedule_cursor_refresh(
    cursor_tooltip_active: bool,
    refresh_state: CursorRefreshState,
) -> bool {
    cursor_tooltip_active && refresh_state == CursorRefreshState::Idle
}

fn schedule_system_npu_capture(
    view: &mut RootView,
    window: &mut Window,
    cx: &mut Context<RootView>,
) {
    if !view.capture_evidence.system_npu_layout_requested() {
        return;
    }
    view.page = TopPage::System;
    view.dashboard.section = SystemSection::Hardware;
    let Some(item) = system_view::graphics_scroll_item(
        view.hardware_rc(),
        view.system_snapshot(),
        view.npu_inventory(),
    ) else {
        return;
    };
    if !view.capture_evidence.schedule_system_npu_scroll() {
        return;
    }
    cx.on_next_frame(window, move |view, window, cx| {
        if view.system_scroll.bounds_for_item(item).is_none() {
            view.capture_evidence.mark_system_npu_scroll_applied(false);
            cx.notify();
            return;
        }
        view.system_scroll.scroll_to_top_of_item(item);
        cx.on_next_frame(window, move |view, _window, cx| {
            let graphics_visible =
                view.system_scroll.top_item() <= item && view.system_scroll.bottom_item() >= item;
            view.capture_evidence
                .mark_system_npu_scroll_applied(graphics_visible);
            cx.notify();
        });
        cx.notify();
    });
}

impl Render for RootView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let presentation = self.presentation_snapshot();
        let ui_size = presentation.appearance.ui_size;
        // All FONT_* tokens resolve from this root-relative scale, including
        // older pages with explicit sizes. Native compositor DPI remains an
        // independent multiplier; row density remains whitespace-only.
        window.set_rem_size(ui_size.body_font_size());
        if matches!(self.input_scope(), super::GpuiInputScope::Content) {
            // Content buttons often close their typed modal state directly. This
            // render-edge cleanup gives those paths the same exact trigger-focus
            // restoration as Escape, the close button, and scrim dismissal.
            restore_modal(window, cx);
        }
        self.ensure_input_modality_key_interceptor(window, cx);
        self.poll_diagnostic_bundle_result();
        schedule_system_npu_capture(self, window, cx);
        if self.capture_evidence.keyboard_focus_requested() {
            // The capture token represents a keyboard-initiated focus state even
            // though the deterministic harness performs the final focus call.
            self.input_modality = InputModality::Keyboard;
            self.page = TopPage::Apps;
            self.pending_search_focus = Some(TopPage::Apps);
            cx.notify();
        }
        if self.capture_evidence.vertical_nav_requested() {
            self.nav_orientation = super::NavOrientation::Vertical;
            self.capture_evidence
                .mark_vertical_nav_ready(self.nav_orientation == super::NavOrientation::Vertical);
            cx.notify();
        }
        let settings_switch_focus = self.capture_evidence.settings_switch_focus_enabled();
        let settings_zero_gray = self.capture_evidence.settings_zero_gray_enabled();
        if settings_switch_focus || settings_zero_gray {
            self.show_settings();
        }
        let settings_focus_id = if settings_switch_focus {
            Some("device-cpu")
        } else if settings_zero_gray {
            Some("gray-zero-values")
        } else {
            None
        };
        let settings_focus_requested = if settings_switch_focus {
            self.capture_evidence.settings_switch_focus_requested()
        } else if settings_zero_gray {
            self.capture_evidence.settings_zero_gray_requested()
        } else {
            false
        };
        if let Some(settings_focus_id) = settings_focus_id
            && settings_focus_requested
        {
            // Strict capture emulates keyboard navigation without dispatching an
            // activation keystroke; the switch value is never changed. The
            // selected settings switch entity owns the focus handle now.
            self.input_modality = InputModality::Keyboard;
            let weak = cx.entity().downgrade();
            window.defer(cx, move |window, cx| {
                let Some(focus) = weak
                    .update(cx, |view, cx| {
                        view.settings_switches
                            .get(settings_focus_id)
                            .map(|state| state.read(cx).focus_handle().clone())
                    })
                    .ok()
                    .flatten()
                else {
                    return;
                };
                focus.focus(window);
                if focus.is_focused(window) {
                    let _ = weak.update(cx, |view, cx| {
                        if settings_focus_id == "device-cpu" {
                            view.capture_evidence.mark_settings_switch_focus_ready();
                        } else {
                            view.capture_evidence.mark_settings_zero_gray_ready();
                        }
                        cx.notify();
                    });
                }
                window.refresh();
            });
        }
        keyboard::defer_pending_search_focus(self, window, cx);
        // Derive one immutable render snapshot from Root-owned input modality
        // and the live platform window chrome state. Every shared focus-ring
        // call receives this same value, without keeping duplicate state in
        // individual controls. The window state (maximized/fullscreen/tiling)
        // drives per-corner CSD rounding; reading it once per frame is cheap.
        let mut t = self.theme;
        t.window_state = window_chrome_state(window);
        // Decoration negotiation: the app asks for `WindowDecorations::Server`
        // (a native titlebar) and reacts to what the compositor actually grants.
        // KDE/KWin, macOS, and Windows grant Server → the OS draws the titlebar +
        // corners and we render NO app chrome. GNOME/Mutter and some tiling WMs
        // force Client (CSD) → we render our own titlebar + window controls and
        // paint rounded corners on the transparent Linux surface. The
        // `decorations_override` hook exists only for render tests (the gpui
        // `TestWindow` always reports Server, so the CSD branch is otherwise
        // unreachable headlessly); production leaves it `None` and reads the
        // live platform value.
        let server_decorations = self
            .decorations_override
            .unwrap_or_else(|| matches!(window.window_decorations(), gpui::Decorations::Server));
        // The system frame owns the outer outline and corners when Server is
        // granted, so painting our own CSD rounding would be double decoration.
        // `window_corner_radius` stays the single source of per-corner radius;
        // this 0/1 factor folds the Server case into the root's corner call
        // sites below (and is inert on CSD, where the radius already collapses
        // to 0 for maximized/fullscreen/tiled states via `corner_enabled`).
        let corner_radius_factor = if server_decorations { 0.0 } else { 1.0 };
        let t = t.with_focus_visible(self.input_modality.shows_focus_ring());
        // Push frame-level state (focus ring) into gpui-component's global
        // ActiveTheme. Theme-level color/font tokens are synced once per
        // change by the theme mutation paths (RootView::set_skin /
        // set_font_choice, startup pre-warm, Settings pills); re-syncing them
        // here every frame would be redundant work.
        let snap = self.system_snapshot_rc().clone();
        let telemetry = self.telemetry.clone();
        let selected = self.selected;
        let sel_pid = self.selected_pid();
        // Snapshot the hover slot once: `.as_ref()` for synchronous helpers (titlebar,
        // sidebar, settings), `.clone()` for the uniform_list row builders (Apps/Services).
        let hovered = self.hovered.clone();
        // U7-1 tooltip: extended info for the hovered process/service (truncated
        // names / long descriptions get a cursor-following tooltip). Derived from
        // the live hover slot — no per-view wiring needed.
        let tooltip_text: Option<String> = match &hovered {
            Some(Hover::Proc(pid)) => self.process_tooltip_text(*pid),
            Some(Hover::Service(name)) => self
                .services()
                .iter()
                .find(|s| s.name == *name)
                .map(|s| s.description.clone())
                .filter(|d| !d.is_empty()),
            Some(Hover::Startup(id)) => self
                .startup_entries()
                .iter()
                .find(|entry| &entry.id == id)
                .map(|e| e.exec.clone())
                .filter(|c| !c.is_empty()),
            // Users page: show the owner's name (rows are short; the tooltip mirrors
            // the row identity like the other list pages).
            Some(Hover::User(name)) => Some(name.clone()),
            // U7-1: sidebar device rows get the device's display name; static chrome
            // (tabs / settings / window controls) get a friendly label. Both derive
            // from the live hover slot — no per-view wiring needed.
            Some(Hover::Device(dev)) => Some(device_label(*dev, &snap)),
            Some(Hover::Static(id)) => static_label(id).map(ToOwned::to_owned),
            None => None,
        };
        // Ordinary table-row hover only changes the row surface on enter/leave.
        // The root cursor listener is needed exclusively for the custom
        // cursor-following tooltip, so keep it completely out of the hot path
        // when the current row has no tooltip text to show.
        let cursor_tooltip_active = tooltip_text.is_some();

        let viewport = window.viewport_size();
        let layout = responsive::PageLayoutBudget::for_frame(viewport, self.nav_orientation);
        // CSD titlebar (drag + title + controls): rendered ONLY in the CSD
        // fallback. The nav strip below is app content and renders in BOTH modes.
        let titlebar: Option<Div> = if server_decorations {
            None
        } else {
            Some(top_bar(
                &t,
                hovered.as_ref(),
                self.tray_controller.is_some(),
                cx,
            ))
        };
        // Page-navigation strip (tabs + gear). Always rendered (below the app
        // titlebar in CSD, below the native titlebar in Server). Tab click flips
        // `self.page`.
        let nav = nav_strip(
            &t,
            self.page,
            self.nav_orientation,
            hovered.as_ref(),
            layout.navigation,
            cx,
        );

        // Body differs per page; Performance is a sidebar+main row, others are padded content.
        let body = self.render_page_body(
            window,
            cx,
            pages::PageBodyFrame {
                theme: &t,
                snapshot: &snap,
                telemetry: &telemetry,
                hovered: hovered.as_ref(),
                selected,
                layout,
                corner_radius_factor,
                selected_pid: sel_pid,
            },
        );

        // Cold-start loading placeholder: while the typed telemetry frame
        // lifecycle is `Collecting`, cached snapshots / process lists may
        // still be incomplete; show a centered status line instead of empty
        // graphs.
        let body = if self.telemetry_frame_state.is_collecting() {
            overlays::cold_start_placeholder(&t, window, cx)
        } else {
            body.debug_selector(|| "tm-telemetry-ready-body".to_string())
        };
        let body = page_viewport(body);

        // Page-switch fade: the body fades in over the hover-class duration
        // (120ms), keyed by the active page so switching pages replays the
        // sweep while an unchanged page stays at full opacity (the keyed
        // animation state persists across frames and settles at delta = 1).
        // Opacity only — text, layout and hit-testing are untouched.
        let body = body.with_animation(
            ("page-fade", self.page as u64),
            Animation::new(tokens::DURATION_HOVER).with_easing(ease_in_out),
            |el, delta| el.opacity(delta),
        );

        let alert_banner = self
            .active_alerts()
            .iter()
            .max_by_key(|alert| alert.severity)
            .cloned()
            .map(|alert| {
                alert_ui::render_banner(&t, alert, self.active_alerts().len(), &snap, cx)
                    .into_any_element()
            });

        let root = div()
            .id("root")
            .size_full()
            .bg(t.window_bg)
            .text_color(t.fg)
            .font(ui_font_with_fallback(&t))
            .font_weight(tokens::FONT_WEIGHT_BODY.into())
            .text_size(ui_size.body_font_size())
            .flex()
            .flex_col()
            .on_mouse_move(cx.listener(move |v, _ev: &MouseMoveEvent, window, cx| {
                // Coalesce the expensive RootView invalidation to one update per
                // animation frame. This is especially important for the
                // virtualized process table: a mouse can generate far more move
                // events than the display can present, while the row highlight
                // itself only changes at an on_hover boundary.
                if cursor_tooltip_active
                    && v.hovered.is_some()
                    && should_schedule_cursor_refresh(cursor_tooltip_active, v.cursor_refresh_state)
                {
                    v.cursor_refresh_state = CursorRefreshState::Scheduled;
                    cx.on_next_frame(window, |_view, _window, cx| {
                        _view.cursor_refresh_state = CursorRefreshState::Idle;
                        if _view.hovered.is_some() {
                            cx.notify();
                        }
                    });
                }
            }))
            .capture_any_mouse_down(cx.listener(RootView::capture_input_modality_mouse_down))
            .capture_key_down(cx.listener(RootView::capture_input_modality_key_down))
            .on_modifiers_changed(cx.listener(RootView::handle_root_modifiers_changed))
            .on_key_down(cx.listener(RootView::handle_root_key_down))
            // Rounded CSD surface: the root's own background carries the window
            // radius; full-bleed chrome children (titlebar/sidebar/scrim) round
            // their own outer corners so nothing paints into the transparent
            // corners. Corners touching a maximized/tiled/fullscreen edge
            // resolve to 0 via `window_corner_radius` (radius is 0 on
            // non-transparent macOS/Windows surfaces, so this stays inert there).
            // Compositor-forced Server decorations zero every corner via
            // `corner_radius_factor` — the system frame draws the outline.
            .rounded_tl(px(
                t.window_corner_radius(WindowCorner::TopLeft) * corner_radius_factor
            ))
            .rounded_tr(px(
                t.window_corner_radius(WindowCorner::TopRight) * corner_radius_factor
            ))
            .rounded_bl(px(
                t.window_corner_radius(WindowCorner::BottomLeft) * corner_radius_factor
            ))
            .rounded_br(px(
                t.window_corner_radius(WindowCorner::BottomRight) * corner_radius_factor
            ))
            .children(titlebar)
            .children(alert_banner);

        let root = match self.nav_orientation {
            super::NavOrientation::Horizontal => root.child(nav).child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .min_w(px(0.0))
                    .w_full()
                    .flex()
                    .flex_col()
                    .child(body),
            ),
            super::NavOrientation::Vertical => root.child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h(px(0.0))
                    .min_w(px(0.0))
                    .w_full()
                    .child(nav)
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .min_h(px(0.0))
                            .min_w(px(0.0))
                            .w_full()
                            .child(body),
                    ),
            ),
        };

        let root = surfaces::compose_active_surface(self, root, &t, window, cx);
        transients::compose(self, root, &t, server_decorations, tooltip_text, window, cx)
    }
}

/// Build the inherited UI font with an explicit CJK fallback. GPUI's
/// `font_family` setter only names the primary family; on Windows that leaves
/// DirectWrite free to pick a different fallback chain per machine. MiSans VF
/// is registered by the app assets, so keeping it in the style makes the
/// effective result deterministic for Chinese copy while preserving the
#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_render_tests.rs"]
mod tests;
