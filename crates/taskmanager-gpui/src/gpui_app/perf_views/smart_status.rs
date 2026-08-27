//! Typed SMART provider-status footer rendering (GPUI-local).
//!
//! The pure status→i18n-key mapping, the SMART-availability projection, and the
//! disk effective-status helper have moved DOWN to
//! [`taskmanager_shell::presentation`] (ADR-027 single-source) so the iced
//! frontend renders the same status text and footer hint as GPUI instead of
//! duplicating the mapping. This module re-exports them under the historical
//! `smart_status::*` path the GPUI call sites still name, and keeps only the
//! GPUI footer element itself (toolkit rendering stays at the renderer edge).

use crate::core::device_state::DeviceStatus;
use crate::gpui_app::theme::{Theme, tokens};
use crate::i18n;
use gpui::{AnyElement, IntoElement, ParentElement, Styled, div};

pub use taskmanager_shell::presentation::{
    device_action_i18n_key, device_status_i18n_key, effective_smart_status, has_smart_fields,
    smart_availability_i18n_key, smart_section_visible,
};

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
            .px(tokens::SPACE_10)
            .py(tokens::SPACE_7)
            .rounded(tokens::small_radius(theme))
            .bg(theme.accent.with_alpha(0.12))
            .text_size(tokens::FONT_12)
            .text_color(theme.fg)
            .child(i18n::t(device_action_i18n_key(status)))
            .into_any_element(),
    )
}
