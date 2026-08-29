//! The Performance-page per-device detail panels: one block per GPU / disk /
//! network adapter / battery in the shared snapshot.

use super::*;
use crate::theme;
use iced::widget::{column, container};
use taskmanager_core::core::device_state::DeviceStatus;
use taskmanager_core::core::metrics::{NetworkAdapterType, NetworkMetrics, SystemSnapshot};
use taskmanager_core::core::power::{BatteryInfo, PowerSupplySnapshot};

use taskmanager_shell::presentation::{
    device_action_i18n_key, device_status_i18n_key, missing_value,
};

pub(crate) mod battery;
pub(crate) use battery::battery_section;

pub(crate) mod disk;
pub(crate) use disk::*;

pub(crate) mod gpu;
pub(crate) use gpu::*;

pub(crate) mod network;
pub(crate) use network::network_section;

mod projection;

pub(crate) mod rates;
pub(crate) use rates::{rate_text_pref, throughput_scale};

/// Wrap a device section's per-device blocks inside the shared panel container.
pub(crate) fn device_rows_panel<'a>(
    rows: Vec<Element<'a, Message, iced::Theme, iced::Renderer>>,
    theme_snapshot: &'a taskmanager_theme::Theme,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    container(column(rows).spacing(12).width(iced::Length::Fill))
        .width(iced::Length::Fill)
        .padding(0)
        .style(move |_| theme::panel_style(theme_snapshot))
        .into()
}

/// The accent-tinted action-hint footer pinned under a device's statistics
/// rail when its status is not Healthy — the iced equivalent of GPUI's
/// `perf_views::smart_status::status_footer`. Returns `None` for a healthy
/// device so the footer space is not occupied unnecessarily. The hint text is
/// the shared `device_action_i18n_key` projection, so iced and GPUI never
/// disagree on which hint to render for a given status.
pub(crate) fn device_status_footer<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    status: DeviceStatus,
) -> Option<Element<'a, Message, iced::Theme, iced::Renderer>> {
    if status == DeviceStatus::Healthy {
        return None;
    }
    let palette = theme_snapshot.palette();
    Some(
        container(text(t(device_action_i18n_key(status))).size(f32::from(tokens::FONT_12)))
            .padding([
                f32::from(taskmanager_theme::tokens::SPACE_7),
                f32::from(taskmanager_theme::tokens::SPACE_10),
            ])
            .style(move |_| {
                use iced::widget::container::Style;
                let accent = taskmanager_theme::iced::color(theme_snapshot.accent);
                Style {
                    background: Some(iced::Background::Color(iced::Color { a: 0.12, ..accent })),
                    border: iced::Border {
                        color: iced::Color::TRANSPARENT,
                        width: 0.0,
                        radius: f32::from(palette.control_radius).into(),
                    },
                    text_color: Some(taskmanager_theme::iced::color(theme_snapshot.fg)),
                    ..Style::default()
                }
            })
            .width(iced::Length::Fill)
            .into(),
    )
}

#[cfg(test)]
#[path = "../../tests/gui/ui/perf_devices/tests.rs"]
mod tests;
