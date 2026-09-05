//! Non-modal and lifecycle layers composed after the active window surface.

use super::super::{RootView, TelemetryWarmupPhase, elements, i18n};
use super::overlays;
use gpui::{
    AnimationExt, Context, Div, InteractiveElement, ParentElement, RenderOnce, Stateful,
    StatefulInteractiveElement, Styled, Window, deferred, div, px,
};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;
use taskmanager_ui::primitives::spinner::Spinner;

pub(super) fn compose(
    view: &mut RootView,
    root: Stateful<Div>,
    theme: &Theme,
    server_decorations: bool,
    tooltip_text: Option<String>,
    window: &mut Window,
    cx: &mut Context<RootView>,
) -> Stateful<Div> {
    let root = compose_pause(view, root, theme, server_decorations, window, cx);
    let root = compose_feedback(view, root, theme, window, cx);
    let root = compose_warmup(view, root, theme, window, cx);
    match (tooltip_text, view.window_surface_open()) {
        (Some(text), false) => root.child(elements::tooltip_overlay(
            theme,
            &text,
            window.mouse_position(),
        )),
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
    let badge = taskmanager_ui::primitives::badge::Badge::new(
        format!("\u{23f8} {}", i18n::t("common.paused")),
        taskmanager_ui::primitives::badge::BadgeTone::Accent,
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
    window: &mut Window,
    cx: &mut Context<RootView>,
) -> Stateful<Div> {
    let Some(toast) = view.local_feedback_toast.clone() else {
        return compose_shell_feedback(view, root, theme, cx);
    };
    let weak = cx.entity().downgrade();
    let card = taskmanager_ui::overlays::toast::Toast::new(toast, theme.palette())
        .on_dismiss(move |_window, cx| {
            let _ = weak.update(cx, |view, cx| {
                view.local_feedback_toast = None;
                cx.notify();
            });
        })
        .render(window, cx);
    root.child(deferred(
        div()
            .absolute()
            .top(px(crate::gpui_app::chrome::titlebar_height(theme) + 40.0))
            .left_0()
            .w_full()
            .flex()
            .justify_center()
            .child(card),
    ))
}

fn compose_shell_feedback(
    view: &RootView,
    root: Stateful<Div>,
    theme: &Theme,
    cx: &mut Context<RootView>,
) -> Stateful<Div> {
    let Some(notice) = view.shell.feedback_notice() else {
        return root;
    };
    let color = match notice.severity() {
        taskmanager_shell::FeedbackSeverity::Info => theme.accent,
        taskmanager_shell::FeedbackSeverity::Success => theme.success,
        taskmanager_shell::FeedbackSeverity::Warning => theme.warning,
        taskmanager_shell::FeedbackSeverity::Error => theme.danger,
    };
    let weak = cx.entity().downgrade();
    let card = div()
        .max_w(px(640.0))
        .px(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_10,
        ))
        .py(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_6,
        ))
        .rounded(px(6.0))
        .bg(taskmanager_ui::theme_binding::fill(theme.card_bg))
        .border_1()
        .border_color(taskmanager_ui::theme_binding::hsla(color.with_alpha(0.45)))
        .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
        .text_color(taskmanager_ui::theme_binding::hsla(theme.fg))
        .flex()
        .items_center()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_8,
        ))
        .child(div().flex_1().child(notice.text().to_owned()))
        .child(
            div()
                .id("dismiss-shell-feedback")
                .debug_selector(|| "dismiss-shell-feedback".to_string())
                .cursor_pointer()
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                .hover(|s| s.text_color(taskmanager_ui::theme_binding::hsla(theme.fg)))
                .on_click(move |_event, _window, cx| {
                    let _ = weak.update(cx, |view, cx| {
                        view.shell.clear_feedback_notice();
                        cx.notify();
                    });
                })
                .child("\u{2715}"),
        );
    root.child(deferred(
        div()
            .absolute()
            .top(px(crate::gpui_app::chrome::titlebar_height(theme) + 40.0))
            .left_0()
            .w_full()
            .flex()
            .justify_center()
            .child(card),
    ))
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
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_8,
        ))
        .child(Spinner::new(theme.palette()).size(18.0))
        .child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_16))
                .font_weight(taskmanager_ui::theme_binding::font_weight(
                    tokens::FONT_WEIGHT_BOLD,
                ))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg))
                .child(headline),
        )
        .child(
            div()
                .max_w(px(420.0))
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                .child(detail),
        );
    let content = if let Some(retry) = retry {
        content.child(
            div()
                .debug_selector(|| "tm-telemetry-warmup-retry".to_string())
                .mt(taskmanager_ui::theme_binding::length(tokens::SPACE_8))
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
            .bg(taskmanager_ui::theme_binding::fill(theme.view_bg))
            .occlude()
            .on_any_mouse_down(|_event, _window, cx| cx.stop_propagation())
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .child(content.with_animation(
                "telemetry-warmup-content",
                taskmanager_ui::theme_binding::appear(),
                |element, delta| element.opacity(delta).mt(px((1.0 - delta) * 6.0)),
            )),
    ))
}
