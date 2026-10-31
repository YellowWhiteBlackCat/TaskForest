//! The monospace tag chips of the process-insights isolation facet: the
//! seccomp mode and the Linux capability bits wear neutral, monospaced tag
//! backgrounds so a permission bit reads as a code token instead of prose.
//!
//! The chip geometry is the shared badge pill; the label is set in the
//! theme's monospace family, and the fill is the theme's own neutral surface
//! token — no color literal enters this module.

use iced::widget::{column, container, row, text};
use iced::{Element, Length};
use taskforest_core::core::process_telemetry::{CapabilityRiskLevel, ProcessCapabilities};
use taskmanager_application::i18n::t;
use taskmanager_theme::{Color, Theme, tokens};
use zhuge_ui_kit_iced::surface::{self, BadgeGeometry};

use crate::app::Message;

/// The chip's box height — the shared badge pill height.
const CHIP_HEIGHT: f32 = 20.0;

/// How many capability bits the row materializes before the overflow marker
/// takes over. The bound keeps the chip row inside the facet's value slot
/// (the bounded label/value grammar), so a full capability set can never
/// push a chip past the panel's right edge.
const CAPABILITY_CHIP_BUDGET: usize = 3;

/// The chip's fill: the theme's neutral surface token.
#[must_use]
pub(crate) fn chip_fill(theme: &Theme) -> Color {
    theme.palette().fg_muted
}

/// The chip's foreground on its fill: the panel surface reads on the neutral
/// chip in both light and dark skins.
#[must_use]
pub(crate) fn chip_foreground(theme: &Theme) -> Color {
    theme.palette().surface
}

/// The risk label of one capability set: the shared localized copy for the
/// measured risk level.
#[must_use]
pub(crate) fn capability_risk_label(level: CapabilityRiskLevel) -> &'static str {
    match level {
        CapabilityRiskLevel::Unknown => t("proc_insights.capabilities_unknown"),
        CapabilityRiskLevel::None => t("proc_insights.capabilities_none"),
        CapabilityRiskLevel::Elevated => t("proc_insights.capabilities_elevated"),
        CapabilityRiskLevel::Critical => t("proc_insights.capabilities_critical"),
    }
}

/// The capability row: the measured risk level plus one monospace chip per
/// effective capability bit. A set longer than the visible budget keeps an
/// explicit `+N` marker, so the reader never sees a silently truncated list.
///
/// The risk level sits on its own line and the chips on the next one: the
/// chip row then owns the full value slot, and the bounded chip budget keeps
/// it inside the panel's right edge.
pub(crate) fn capability_chips<'a>(
    theme_snapshot: &'a Theme,
    capabilities: &ProcessCapabilities,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let effective = capabilities.effective_capabilities().unwrap_or_default();
    let mut chips = row![].spacing(6);
    for capability in effective.iter().take(CAPABILITY_CHIP_BUDGET) {
        chips = chips.push(mono_chip(theme_snapshot, capability.as_str()));
    }
    if effective.len() > CAPABILITY_CHIP_BUDGET {
        chips = chips.push(mono_chip(
            theme_snapshot,
            format!("+{}", effective.len() - CAPABILITY_CHIP_BUDGET),
        ));
    }
    column![
        text(capability_risk_label(capabilities.risk_level())).size(f32::from(tokens::FONT_12)),
        chips,
    ]
    .spacing(4)
    .into()
}

/// One monospace tag chip: a neutral, fully rounded capsule carrying a code
/// token (a seccomp mode, a capability bit, a "+N" overflow marker).
pub(crate) fn mono_chip<'a>(
    theme_snapshot: &'a Theme,
    label: impl Into<String>,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let fill = crate::theme_binding::color(chip_fill(theme_snapshot));
    let foreground = crate::theme_binding::color(chip_foreground(theme_snapshot));
    let font = crate::theme_binding::mono_font(theme_snapshot);
    surface::badge(
        label,
        BadgeGeometry {
            size: f32::from(tokens::FONT_CAPTION),
            font: Some(font),
            padding: [0.0, f32::from(tokens::SPACE_8)].into(),
            height: Length::Fixed(CHIP_HEIGHT),
            centered: true,
        },
        move |_| container::Style {
            background: Some(fill.into()),
            text_color: Some(foreground),
            border: iced::Border {
                color: iced::Color::TRANSPARENT,
                width: 0.0,
                radius: (CHIP_HEIGHT / 2.0).into(),
            },
            ..container::Style::default()
        },
    )
}
