//! The per-battery Performance-page detail panel (charge-% trend + the
//! honest scalar rows), extracted from [`super`] so the device-page module
//! stays under the source-size budget. The readiness / title / summary /
//! section / block quintet mirrors the GPU / disk / network panels.

use super::*;
// Explicit like the device-page module: disambiguates the `column!` macro
// (iced::widget) from the prelude's, which a bare glob leaves ambiguous.
use iced::widget::{column, container};
use taskmanager_theme::tokens;

// --- Battery --------------------------------------------------------------

/// The Performance-page per-battery panel readiness, mirroring the GPU/disk/
/// network panels' Loading/Empty/Ready states. `None` (no power-supply snapshot
/// observed yet) is Loading; `Some` with an empty battery vector is Empty (an
/// honest "no battery" state, not a hidden zero); otherwise Ready. Private like
/// its siblings — [`tables::ListState`] is `pub(in crate::ui)`.
#[must_use]
pub(crate) fn battery_section_state(
    power_supplies: Option<&PowerSupplySnapshot>,
) -> tables::ListState {
    match power_supplies {
        None => tables::ListState::Loading,
        Some(snapshot) if snapshot.batteries.is_empty() => tables::ListState::Empty,
        Some(_) => tables::ListState::Ready,
    }
}

/// One battery's display identity: the model name when known, else the provider
/// display name, else the neutral localized "Battery" label — never an empty
/// heading, mirroring [`gpu_title`]/[`disk_title`]/[`network_title`].
#[must_use]
pub(crate) fn battery_title(battery: &BatteryInfo, index: usize) -> String {
    let name = (!battery.model_name.trim().is_empty())
        .then(|| battery.model_name.trim().to_string())
        .or_else(|| {
            (!battery.display_name.trim().is_empty())
                .then(|| battery.display_name.trim().to_string())
        });
    match name {
        Some(name) => format!("{}: {name}", t("common.battery")),
        // No identity → fall back to the per-battery index ("Battery 0") so two
        // anonymous batteries stay distinguishable (mirrors GPUI render_battery).
        None => format!("{} {index}", t("common.battery")),
    }
}

/// Project one battery's honest scalar readouts as label/value rows for the
/// Performance page, mirroring `gpui_app::perf_views::dynamic::render_battery`.
/// Charge is the headline: an unknown capacity renders an honest dash, NEVER a
/// fabricated 0% (the same rule as the GPU/disk percent readouts). The status
/// string conveys charge vs discharge direction; the rate row carries the
/// magnitude. Voltage, cycles, technology and manufacturer are shown only when
/// the provider supplied them so a missing node cannot masquerade as a measured
/// zero. Health derives only from the typed µWh pair, and each runtime
/// estimate appears only when the native source reported one under its status
/// gate — an unavailable estimate is an absent row, never "0%"/"00h 00m".
#[must_use]
pub(crate) fn battery_summary_lines(battery: &BatteryInfo) -> Vec<(String, String)> {
    let observed = super::projection::BatteryObservation::from(battery);
    let mut rows = vec![
        (
            t("battery.capacity").to_string(),
            observed
                .capacity_pct
                .map(|value| format!("{value}%"))
                .unwrap_or_else(missing_value),
        ),
        (t("battery.status").to_string(), battery.status.clone()),
    ];
    if let Some(power) = observed.power_w {
        rows.push((t("battery.power").to_string(), format!("{power:.1} W")));
    }
    if let Some(voltage_uv) = observed.voltage_uv {
        rows.push((
            t("battery.voltage").to_string(),
            format!("{:.2} V", voltage_uv as f64 / 1_000_000.0),
        ));
    }
    if let Some(cycles) = observed.cycle_count {
        rows.push((t("battery.cycles").to_string(), cycles.to_string()));
    }
    if let Some(health) = observed.health_pct {
        rows.push((t("battery.health").to_string(), format!("{health:.1}%")));
    }
    if let Some(secs) = observed.time_to_full_secs {
        rows.push((t("battery.time_to_full").to_string(), duration(secs as u64)));
    }
    if let Some(secs) = observed.time_to_empty_secs {
        rows.push((
            t("battery.time_to_empty").to_string(),
            duration(secs as u64),
        ));
    }
    if !battery.technology.trim().is_empty() {
        rows.push((
            t("battery.technology").to_string(),
            battery.technology.trim().to_string(),
        ));
    }
    if !battery.manufacturer.trim().is_empty() {
        rows.push((
            t("battery.manufacturer").to_string(),
            battery.manufacturer.trim().to_string(),
        ));
    }
    rows
}

/// The Performance-page per-battery panel: one block per battery in the shared
/// power-supply snapshot, each block topped by a per-battery charge-% mini-graph
/// (fixed 0..100) plotted from that battery's OWN per-device window. No snapshot
/// → the collecting state; a snapshot with no battery → the honest empty line
/// (no battery detected); otherwise each battery's title, its charge-% trend,
/// then its honest scalar rows — the same shape as the GPU/disk/network panels.
pub(crate) fn battery_section(
    app: &crate::IcedApp,
    index: usize,
) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let power_supplies = app.shell.projection().power_supplies.as_ref();
    let theme_snapshot = app.theme();
    let color = theme::color(theme_snapshot.battery);
    let rows = match (battery_section_state(power_supplies), power_supplies) {
        (tables::ListState::Loading, _) => {
            vec![tables::message_panel(
                theme_snapshot,
                t("common.collecting_telemetry"),
            )]
        }
        (tables::ListState::Empty, _) => {
            vec![tables::message_panel(theme_snapshot, t("battery.empty"))]
        }
        (tables::ListState::Ready, Some(snapshot)) => match snapshot.batteries.get(index) {
            Some(battery) => vec![battery_block(
                app,
                battery,
                color,
                theme_snapshot,
                app.compact_layout(),
                index,
                true,
            )],
            None => vec![tables::message_panel(theme_snapshot, t("battery.empty"))],
        },
        (tables::ListState::Ready, None) => {
            vec![tables::message_panel(
                theme_snapshot,
                t("common.collecting_telemetry"),
            )]
        }
    };
    device_rows_panel(rows, theme_snapshot)
}

/// One battery's rendered block: the device title line, a per-battery charge-%
/// mini-graph (the device's OWN window, fixed 0..100), then its scalar rows —
/// mirroring the GPU/disk/network block shape.
fn battery_block<'a>(
    app: &crate::IcedApp,
    battery: &BatteryInfo,
    color: iced::Color,
    theme_snapshot: &'a taskmanager_theme::Theme,
    compact: bool,
    index: usize,
    smooth: bool,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let samples = app.cached_battery_series(&battery.id);
    let mut graphs = vec![device_chart::device_mini_graph_fill(
        samples,
        device_chart::DeviceMetricScale::Percent,
        color,
        t("battery.capacity").to_string(),
        theme_snapshot,
        compact,
        device_chart::GraphPrefs {
            smooth,
            max_override: None,
            // Both the charge main graph and the power graph below are
            // hover-interactive (crosshair + readout pill at any height).
            hover: true,
        },
    )];
    let power_samples = app.cached_battery_power_series(&battery.id);
    if !power_samples.is_empty() {
        graphs.push(device_chart::device_mini_graph_with_height(
            power_samples,
            device_chart::DeviceMetricScale::Watts,
            color,
            t("battery.power_graph").to_string(),
            theme_snapshot,
            device_chart::SECONDARY_DEVICE_CHART_HEIGHT,
            device_chart::GraphPrefs {
                smooth,
                max_override: None,
                hover: true,
            },
        ));
    }
    let panel = perf_layout::main_with_stats(
        theme_snapshot,
        battery_title(battery, index),
        t("battery.charge_graph").to_string(),
        graphs,
        battery_summary_lines(battery),
        compact,
        perf_layout::DetailExtent::for_scroll_parent(compact),
    );
    // A non-healthy battery gets an accent-tinted action-hint footer below the
    // stats (mirrors GPUI's status_footer): Stale / PermissionDenied /
    // MissingTool surfaces the cause instead of reading like a healthy battery.
    match device_status_footer(theme_snapshot, battery.device_state.status) {
        Some(footer) => column![panel, footer].spacing(8).into(),
        None => panel,
    }
}

/// The accent-tinted action-hint footer shown beneath a device's stats when its
/// status is not Healthy — the iced equivalent of GPUI's
/// `perf_views::smart_status::status_footer`. Returns `None` for a healthy
/// device so the footer space is not occupied unnecessarily. The hint text is
/// the shared `device_action_i18n_key` projection, so iced and GPUI never
/// disagree on which hint to render for a given status.
fn device_status_footer<'a>(
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
                let accent = theme::color(theme_snapshot.accent);
                Style {
                    background: Some(iced::Background::Color(iced::Color { a: 0.12, ..accent })),
                    border: iced::Border {
                        color: iced::Color::TRANSPARENT,
                        width: 0.0,
                        radius: f32::from(palette.control_radius).into(),
                    },
                    text_color: Some(theme::color(theme_snapshot.fg)),
                    ..Style::default()
                }
            })
            .width(iced::Length::Fill)
            .into(),
    )
}
