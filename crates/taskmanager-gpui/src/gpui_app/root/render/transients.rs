//! Non-modal and lifecycle layers composed after the active window surface.

use super::super::{RootView, TelemetryWarmupPhase, elements, i18n};
use super::overlays;
use gpui::{
    AnimationExt, Context, Div, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    Stateful, StatefulInteractiveElement, Styled, Window, deferred, div, prelude::FluentBuilder,
    px,
};
use taskmanager_shell::FeedbackSeverity;
use taskmanager_theme::GlassTier;
use taskmanager_theme::tokens;
use taskmanager_theme::{Theme, WindowControls};
use taskmanager_ui::icons_binding::icon;
use taskmanager_ui::overlays::toast::Toast;
use taskmanager_ui::primitives::badge::Badge;
use taskmanager_ui::primitives::badge::BadgeTone;
use taskmanager_ui::primitives::spinner::Spinner;
use taskmanager_ui::theme_binding::appear;
use taskmanager_ui::theme_binding::definite_length;
use taskmanager_ui::theme_binding::fill;
use taskmanager_ui::theme_binding::font_size;
use taskmanager_ui::theme_binding::font_weight;
use taskmanager_ui::theme_binding::glass_fill;
use taskmanager_ui::theme_binding::glass_shadow;
use taskmanager_ui::theme_binding::hsla;
use taskmanager_ui::theme_binding::length;
use taskmanager_ui::theme_binding::rem_length;
use taskmanager_ui::theme_binding::rem_value;
use taskmanager_ui_contract::IconId;
use taskmanager_ui_contract::navigation::NavOrientation;

const FEEDBACK_EDGE_INSET: f32 = 16.0;
const FEEDBACK_CSD_TOP: f32 = 3.0;
const FEEDBACK_NATIVE_NAV_TOP: f32 = 6.0;
const FEEDBACK_TITLE_CLEARANCE: f32 = 180.0;
const FEEDBACK_MAX_WIDTH: f32 = 420.0;
const FEEDBACK_MIN_WIDTH: f32 = 240.0;
const FEEDBACK_VIEWPORT_WIDTH_FRACTION: f32 = 0.36;

/// The frame facts the transient overlay layer reads: live viewport width and
/// whether server decorations own the titlebar. Bundled so the compose entry
/// point keeps a small argument list while overlays share the same frame facts.
#[derive(Clone, Copy, Debug)]
pub(super) struct TransientFrame {
    pub(super) viewport_width: f32,
    pub(super) server_decorations: bool,
    pub(super) nav_orientation: NavOrientation,
    pub(super) navigation_width: f32,
}

pub(super) fn compose(
    view: &mut RootView,
    root: Stateful<Div>,
    theme: &Theme,
    frame: TransientFrame,
    tooltip_text: Option<String>,
    window: &mut Window,
    cx: &mut Context<RootView>,
) -> Stateful<Div> {
    let root = compose_pause(view, root, theme, frame.server_decorations, window, cx);
    let root = compose_feedback(view, root, theme, frame, window, cx);
    let root = compose_warmup(view, root, theme, window, cx);
    match (tooltip_text, view.window_surface_open()) {
        (Some(text), false) => {
            // The hint is in the tree: certify it only now, and only for the
            // strict tooltip scenario. A marker emitted anywhere earlier could
            // be true while the overlay was absent (the slot is published by
            // this same frame), which is exactly the false pass a pixel
            // receipt cannot catch.
            if view.capture_evidence.tooltip_overlay_requested() {
                view.capture_evidence.mark_tooltip_overlay_ready(true);
            }
            root.child(elements::tooltip_overlay(
                theme,
                &text,
                window.mouse_position(),
            ))
        }
        (Some(_) | None, true) | (None, false) => root,
    }
}

fn compose_pause(
    view: &RootView,
    root: Stateful<Div>,
    theme: &Theme,
    server_decorations: bool,
    window: &mut Window,
    cx: &mut Context<RootView>,
) -> Stateful<Div> {
    if !view.telemetry_refresh_policy.is_paused() {
        return root;
    }
    let badge = Badge::new(
        format!("\u{23f8} {}", i18n::t("common.paused")),
        BadgeTone::Accent,
        theme.palette(),
    )
    .render(window, cx);
    let top = if server_decorations {
        px(6.0)
    } else {
        px(crate::gpui_app::chrome::titlebar_height(theme) + 6.0)
    };
    root.child(deferred(
        div().absolute().top(top).right(px(48.0)).child(badge),
    ))
}

fn compose_feedback(
    view: &mut RootView,
    root: Stateful<Div>,
    theme: &Theme,
    frame: TransientFrame,
    window: &mut Window,
    cx: &mut Context<RootView>,
) -> Stateful<Div> {
    let Some(toast) = view.local_feedback_toast.clone() else {
        return compose_shell_feedback(view, root, theme, frame, cx);
    };
    let compact_indicator = feedback_compact_indicator(frame);
    let weak = cx.entity().downgrade();
    let card = Toast::new(toast, theme.palette())
        .single_line()
        .indicator(compact_indicator)
        .on_dismiss(move |_window, cx| {
            let _ = weak.update(cx, |view, cx| {
                view.local_feedback_toast = None;
                cx.notify();
            });
        })
        .render(window, cx);
    compose_feedback_overlay(root, card, frame, theme)
}

fn compose_shell_feedback(
    view: &mut RootView,
    root: Stateful<Div>,
    theme: &Theme,
    frame: TransientFrame,
    cx: &mut Context<RootView>,
) -> Stateful<Div> {
    let Some(notice) = view.shell.feedback_notice() else {
        return root;
    };
    let compact_indicator = feedback_compact_indicator(frame);
    // The card is in the tree from here on: certify the GLASS-02 capture only
    // after the real shell notice has mounted, never on the mere request.
    if view.capture_evidence.feedback_overlay_requested() {
        view.capture_evidence.mark_feedback_overlay_ready(true);
    }
    let severity_color = match notice.severity() {
        FeedbackSeverity::Info => theme.accent,
        FeedbackSeverity::Success => theme.success,
        FeedbackSeverity::Warning => theme.warning,
        FeedbackSeverity::Error => theme.danger,
    };
    let weak = cx.entity().downgrade();
    let layer = theme.glass_card();
    let message = div()
        .flex_1()
        .min_w(px(0.0))
        .overflow_hidden()
        .truncate()
        .child(notice.text().to_owned());
    let dismiss = div()
        .id("dismiss-shell-feedback")
        .debug_selector(|| "dismiss-shell-feedback".to_string())
        .flex_shrink_0()
        .cursor_pointer()
        .text_color(hsla(theme.fg_dim))
        .hover(|s| s.text_color(hsla(theme.fg)))
        .on_click(move |_event, _window, cx| {
            let _ = weak.update(cx, |view, cx| {
                view.shell.clear_feedback_notice();
                cx.notify();
            });
        })
        .child("\u{2715}");
    let card = div()
        .max_w(px(640.0))
        .px(definite_length(tokens::SPACE_10))
        .py(definite_length(tokens::SPACE_6))
        .rounded(rem_length(tokens::card_radius(theme)))
        .bg(glass_fill(theme, GlassTier::Card))
        .border(gpui::rems(GlassTier::Card.edge_width_rem()))
        .border_color(hsla(layer.edge))
        .shadow(glass_shadow(theme, GlassTier::Card))
        .text_size(font_size(tokens::FONT_12))
        .text_color(hsla(theme.fg))
        .flex()
        .min_w(px(0.0))
        .items_center()
        .gap(definite_length(tokens::SPACE_8))
        .child(
            icon(feedback_severity_icon(notice.severity()))
                .size(rem_value(16.0))
                .text_color(hsla(severity_color)),
        )
        .when(!compact_indicator, |card| {
            card.child(message).child(dismiss)
        });
    compose_feedback_overlay(root, card, frame, theme)
}

/// Keep the notice's severity visible without tinting the whole glass card.
/// The icon and its theme ink carry the semantic tone; the material stays the
/// shared neutral card tier.
pub(super) fn feedback_severity_icon(severity: FeedbackSeverity) -> IconId {
    match severity {
        FeedbackSeverity::Info => IconId::Health,
        FeedbackSeverity::Success => IconId::CircleCheck,
        FeedbackSeverity::Warning => IconId::TriangleAlert,
        FeedbackSeverity::Error => IconId::Alert,
    }
}

/// Compose feedback as a bounded, deferred corner layer. The wrapper is
/// absolute so mounting or dismissing a notice never changes body geometry;
/// its width is viewport-derived so long paths cannot become the layout
/// authority for the card.
fn compose_feedback_overlay(
    root: Stateful<Div>,
    card: impl IntoElement,
    frame: TransientFrame,
    theme: &Theme,
) -> Stateful<Div> {
    let available_width = (frame.viewport_width - FEEDBACK_EDGE_INSET * 2.0).max(0.0);
    let requested_width = (frame.viewport_width * FEEDBACK_VIEWPORT_WIDTH_FRACTION)
        .clamp(FEEDBACK_MIN_WIDTH, FEEDBACK_MAX_WIDTH)
        .min(available_width);
    let native_vertical =
        frame.server_decorations && frame.nav_orientation == NavOrientation::Vertical;
    let width = if native_vertical {
        requested_width.min((frame.navigation_width - FEEDBACK_EDGE_INSET * 2.0 - 4.0).max(0.0))
    } else if frame.server_decorations {
        requested_width
    } else {
        let title_safe_width = ((frame.viewport_width - FEEDBACK_TITLE_CLEARANCE) / 2.0
            - FEEDBACK_EDGE_INSET)
            .max(0.0);
        requested_width.min(title_safe_width)
    };
    let top = if frame.server_decorations {
        FEEDBACK_NATIVE_NAV_TOP
    } else {
        FEEDBACK_CSD_TOP
    };
    let overlay = div()
        .debug_selector(|| "tm-feedback-overlay".to_string())
        .absolute()
        .top(px(top))
        .max_w(px(width))
        .flex()
        .child(card);
    let overlay = match (
        native_vertical,
        frame.server_decorations,
        theme.window_controls,
    ) {
        (true, _, _) => overlay.left(px(FEEDBACK_EDGE_INSET)),
        (false, true, _) | (false, false, WindowControls::TrafficLight) => {
            overlay.right(px(FEEDBACK_EDGE_INSET))
        }
        (false, false, _) => overlay.left(px(FEEDBACK_EDGE_INSET)),
    };
    root.child(deferred(overlay))
}

fn feedback_compact_indicator(frame: TransientFrame) -> bool {
    frame.server_decorations
        && frame.nav_orientation == NavOrientation::Vertical
        && frame.navigation_width < 100.0
}

fn compose_warmup(
    view: &mut RootView,
    root: Stateful<Div>,
    theme: &Theme,
    window: &mut Window,
    cx: &mut Context<RootView>,
) -> Stateful<Div> {
    if !view.telemetry_frame_state.is_collecting() {
        return root;
    }
    let phase = view.telemetry_warmup_phase();
    let retry = phase
        .allows_retry()
        .then(|| overlays::warmup_retry_button(view, theme, window, cx));
    let headline = if phase.allows_retry() {
        i18n::t("common.telemetry_waiting")
    } else {
        i18n::t("common.loading")
    };
    let detail = match phase {
        TelemetryWarmupPhase::Collecting => i18n::t("common.telemetry_warming_up"),
        TelemetryWarmupPhase::Slow => i18n::t("common.telemetry_warming_up_slow"),
        TelemetryWarmupPhase::Retryable => i18n::t("common.telemetry_warming_up_retry"),
    };
    let content = div()
        .flex()
        .flex_col()
        .items_center()
        .gap(definite_length(tokens::SPACE_8))
        .child(Spinner::new(theme.palette()).size(f32::from(tokens::ICON_STANDALONE)))
        .child(
            div()
                .text_size(font_size(tokens::FONT_16))
                .font_weight(font_weight(tokens::FONT_WEIGHT_BOLD))
                .text_color(hsla(theme.fg))
                .child(headline),
        )
        .child(
            div()
                .max_w(px(420.0))
                .text_size(font_size(tokens::FONT_12))
                .text_color(hsla(theme.fg_dim))
                .child(detail),
        );
    let content = if let Some(retry) = retry {
        content.child(
            div()
                .debug_selector(|| "tm-telemetry-warmup-retry".to_string())
                .mt(length(tokens::SPACE_8))
                .child(retry),
        )
    } else {
        content
    };
    root.child(deferred(
        div()
            .absolute()
            .inset_0()
            .debug_selector(|| "tm-telemetry-warmup".to_string())
            .bg(fill(theme.view_bg))
            .occlude()
            .on_any_mouse_down(|_event, _window, cx| cx.stop_propagation())
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .child(content.with_animation(
                "telemetry-warmup-content",
                appear(),
                |element, delta| element.opacity(delta).mt(px((1.0 - delta) * 6.0)),
            )),
    ))
}
