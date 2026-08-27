//! The Performance-page per-device detail panels: one block per GPU / disk /
//! network adapter / battery in the shared snapshot.

use super::*;
use crate::theme;
use iced::widget::{column, container};
use taskmanager_application::{
    BatteryInfo, DeviceStatus, NetworkAdapterType, NetworkMetrics, PowerSupplySnapshot,
    SystemSnapshot,
};
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

#[cfg(test)]
#[path = "../../tests/gui/ui/perf_devices/tests.rs"]
mod tests;
