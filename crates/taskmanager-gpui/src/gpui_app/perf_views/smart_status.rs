//! Typed SMART provider-status footer rendering (GPUI-local).
//!
//! The pure status→i18n-key mapping, the SMART-availability projection, and
//! the disk effective-status helper live in
//! [`taskmanager_shell::presentation`] (ADR-027 single-source) so the iced
//! frontend renders the same status text and footer hint as GPUI; both
//! frontends import them from that owner path. This module keeps only the
//! GPUI footer element itself (toolkit rendering stays at the renderer
//! edge).

use crate::gpui_app::elements;
use crate::gpui_app::root::RootView;
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div,
};
use taskmanager_application::i18n;
use taskmanager_core::core::device_state::DeviceStatus;
use taskmanager_core::core::metrics::DiskMetrics;
use taskmanager_shell::presentation::device_action_i18n_key;
use taskmanager_shell::presentation::smart::smart_section_visible;
use taskmanager_shell::presentation::{effective_smart_status, has_smart_fields};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;
use taskmanager_ui::theme_binding::absolute;
use taskmanager_ui::theme_binding::definite_length;
use taskmanager_ui::theme_binding::fill;
use taskmanager_ui::theme_binding::font_size;
use taskmanager_ui::theme_binding::hsla;

/// The actionable status footer for a non-healthy device status: an accent-tinted
/// callout carrying the shared action hint ([`device_action_i18n_key`]). Returns
/// `None` for a healthy device so callers render no banner. The shared key
/// selection means iced's banner (rendered by the iced frontend) reads exactly
/// the same hint.
pub(super) fn status_footer(theme: &Theme, status: DeviceStatus) -> Option<AnyElement> {
    if status == DeviceStatus::Healthy {
        return None;
    }
    Some(
        div()
            .px(definite_length(tokens::SPACE_10))
            .py(definite_length(tokens::SPACE_7))
            .rounded(absolute(tokens::small_radius(theme)))
            .bg(fill(theme.accent.with_alpha(0.12)))
            .text_size(font_size(tokens::FONT_12))
            .text_color(hsla(theme.fg))
            .child(i18n::t(device_action_i18n_key(status)))
            .into_any_element(),
    )
}

pub(super) fn disk_footer(
    theme: &Theme,
    disk: &DiskMetrics,
    index: usize,
    cx: &mut Context<RootView>,
) -> Option<AnyElement> {
    if !smart_section_visible(disk) {
        return None;
    }
    let status = effective_smart_status(disk);
    if status != DeviceStatus::Healthy || !has_smart_fields(disk) {
        return status_footer(theme, status);
    }
    Some(
        div()
            .id("disk-smart-btn")
            .focusable()
            .tab_stop(true)
            .focus(elements::focus_ring(theme))
            .cursor_pointer()
            .on_click(cx.listener(move |view, _event, _window, cx| {
                view.show_disk_smart(index);
                cx.notify();
            }))
            .child(
                div()
                    .text_size(font_size(tokens::FONT_12))
                    .text_color(hsla(theme.accent))
                    .child(i18n::t("disk.smart_health")),
            )
            .into_any_element(),
    )
}
