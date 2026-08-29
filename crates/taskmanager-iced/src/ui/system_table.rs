//! Typed System facts and telemetry projection for the Iced frontend.

use iced::widget::{column, row, scrollable, text};
use iced::{Element, Length};
use taskmanager_application::i18n::t;
use taskmanager_core::core::hardware::{DisplayInfo, HardwareInfo};
use taskmanager_core::core::metrics::SystemSnapshot;
use taskmanager_core::core::npu::NpuInventorySnapshot;

use taskmanager_shell::presentation::{duration, missing_value};
use taskmanager_theme::tokens;

use super::components::{key_value_rows, message_panel, titled_card};
use super::tables::ListState;
use crate::IcedApp;
use crate::app::Message;

mod npu;
pub(crate) use npu::{NpuDeviceViewModel, npu_device_view_models};

/// Render System facts and the independent telemetry summary.
pub(super) fn system_page(app: &IcedApp) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let shell = &app.shell;
    let theme_snapshot = app.theme();
    let hardware = shell.projection().hardware.as_ref();
    let hardware_rows = hardware.map(hardware_info_rows).unwrap_or_default();
    let hardware_panel: Element<'_, Message, iced::Theme, iced::Renderer> =
        match hardware_list_state(hardware, &hardware_rows) {
            ListState::Loading => message_panel(theme_snapshot, t("hardware.inventory_waiting")),
            ListState::Empty => message_panel(theme_snapshot, t("hardware.no_facts")),
            ListState::Ready => info_panel(theme_snapshot, t("common.hardware"), &hardware_rows),
        };

    let telemetry_panel: Element<'_, Message, iced::Theme, iced::Renderer> =
        match shell.projection().snapshot.as_ref() {
            Some(snapshot) => {
                let rows = telemetry_rows(snapshot);
                info_panel(theme_snapshot, t("system.telemetry_summary"), &rows)
            }
            None => message_panel(theme_snapshot, t("common.telemetry_waiting")),
        };
    let npu_models = npu_device_view_models(shell.projection().npu_inventory.as_ref());
    let npu_panels = npu_models
        .iter()
        .map(|model| npu_info_panel(theme_snapshot, model));
    let content = std::iter::once(
        // Dashboard segment leads the System page (summary card + history
        // window selection + alert mirror); the window pills publish
        // frontend-local state reduced in `reduce_performance_message`.
        super::system_dashboard::render_system_dashboard(app, app.system_dashboard_window),
    )
    .chain(std::iter::once(hardware_panel))
    .chain(npu_panels)
    .chain(std::iter::once(telemetry_panel))
    .collect::<Vec<_>>();

    let header_row = row![
        text(t("system.title")).size(f32::from(tokens::FONT_16)),
        crate::focus::dynamic_button(
            theme_snapshot,
            crate::app::FocusTarget::AboutCopyDetails,
            t("about.copy_details").to_string(),
            Message::CopyTextToClipboard {
                label: "System Details".to_string(),
                text: format_system_spec_export(
                    hardware,
                    shell.projection().snapshot.as_ref(),
                    shell.projection().npu_inventory.as_ref(),
                ),
            },
            false,
        )
    ]
    .spacing(12)
    .align_y(iced::Alignment::Center);

    column![
        header_row,
        scrollable(column(content).spacing(12)).height(Length::Fill),
    ]
    .spacing(8)
    .height(Length::Fill)
    .into()
}

pub(crate) fn format_system_spec_export(
    hardware: Option<&HardwareInfo>,
    snapshot: Option<&SystemSnapshot>,
    npu_inventory: Option<&NpuInventorySnapshot>,
) -> String {
    let mut lines = Vec::new();
    lines.push("# System Specifications".to_string());
    if let Some(hw) = hardware {
        for row in hardware_info_rows(hw) {
            lines.push(format!("- {}: {}", row.label, row.value));
        }
    }
    if let Some(snap) = snapshot {
        for row in telemetry_rows(snap) {
            lines.push(format!("- {}: {}", row.label, row.value));
        }
    }
    for model in npu_device_view_models(npu_inventory) {
        lines.push(format!("## {}", model.title));
        for row in model.rows {
            lines.push(format!("- {}: {}", row.label, row.value));
        }
    }
    lines.join("\n")
}

fn npu_info_panel<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    model: &NpuDeviceViewModel,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    iced::widget::container(
        column![
            text(model.title.clone()).size(f32::from(tokens::FONT_14)),
            key_value_rows(
                model
                    .rows
                    .iter()
                    .map(|row| (row.label.clone(), row.value.clone()))
                    .collect(),
            )
        ]
        .spacing(6),
    )
    .style(move |_| crate::theme::card_style(theme_snapshot))
    .padding(10)
    .width(Length::Fill)
    .into()
}

fn info_panel<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    title: &'static str,
    rows: &[SystemInfoRow],
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    titled_card(
        theme_snapshot,
        title,
        key_value_rows(
            rows.iter()
                .map(|row| (row.label.clone(), row.value.clone()))
                .collect(),
        ),
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SystemInfoRow {
    pub(crate) label: String,
    pub(crate) value: String,
}

pub(super) fn hardware_list_state(
    hardware: Option<&HardwareInfo>,
    rows: &[SystemInfoRow],
) -> ListState {
    match hardware {
        None => ListState::Loading,
        Some(_) if rows.is_empty() => ListState::Empty,
        Some(_) => ListState::Ready,
    }
}

pub(crate) fn hardware_info_rows(hardware: &HardwareInfo) -> Vec<SystemInfoRow> {
    let mut rows = Vec::new();
    push_text(
        &mut rows,
        t("system.field.os_name"),
        hardware.os_name.as_deref(),
    );
    push_text(
        &mut rows,
        t("system.field.os_version"),
        hardware.os_version.as_deref(),
    );
    push_text(
        &mut rows,
        t("system.kernel"),
        hardware.kernel_version.as_deref(),
    );
    push_text(
        &mut rows,
        t("system.field.kernel_build"),
        hardware.kernel_build.as_deref(),
    );
    push_text(
        &mut rows,
        t("system.hostname"),
        hardware.hostname.as_deref(),
    );
    push_text(
        &mut rows,
        t("system.field.package_manager"),
        hardware.package_manager.as_deref(),
    );
    push_text(
        &mut rows,
        t("system.field.package_manager_version"),
        hardware.package_manager_version.as_deref(),
    );
    push_value(
        &mut rows,
        t("system.field.package_count"),
        hardware.package_count.map(|count| count.to_string()),
    );
    push_text(
        &mut rows,
        t("system.field.desktop"),
        hardware.desktop_environment.as_deref(),
    );
    push_text(
        &mut rows,
        t("system.field.desktop_version"),
        hardware.desktop_environment_version.as_deref(),
    );
    push_text(
        &mut rows,
        t("system.field.windowing"),
        hardware.windowing_system.as_deref(),
    );
    let window_manager = hardware
        .window_manager
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(|name| {
            hardware
                .window_manager_version
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .map_or_else(|| name.to_owned(), |version| format!("{name} {version}"))
        });
    push_text(
        &mut rows,
        t("system.field.window_manager"),
        window_manager.as_deref(),
    );
    push_text(
        &mut rows,
        t("system.field.compositor_backend"),
        hardware.compositor_backend.as_deref(),
    );
    push_text(
        &mut rows,
        t("system.field.virtual_terminal"),
        hardware.virtual_terminal.as_deref(),
    );
    push_text(
        &mut rows,
        t("system.field.shell"),
        hardware.shell.as_deref(),
    );
    push_text(
        &mut rows,
        t("system.field.terminal"),
        hardware.terminal.as_deref(),
    );
    push_text(
        &mut rows,
        t("system.field.terminal_version"),
        hardware.terminal_version.as_deref(),
    );
    push_text(
        &mut rows,
        t("system.field.locale"),
        hardware.locale.as_deref(),
    );
    push_text(
        &mut rows,
        t("system.field.init_system"),
        hardware.init_system.as_deref(),
    );
    for display in &hardware.displays {
        push_value(
            &mut rows,
            t("system.display"),
            Some(display_summary(display)),
        );
    }
    push_text(&mut rows, t("common.cpu"), hardware.cpu_brand.as_deref());
    push_value(&mut rows, t("common.logical_cores"), hardware.cpu_cores);
    push_value(&mut rows, t("system.field.sockets"), hardware.sockets);
    push_value(
        &mut rows,
        t("system.field.installed_memory"),
        hardware.total_memory_mb.map(|value| format!("{value} MiB")),
    );
    push_value(
        &mut rows,
        t("system.field.base_frequency"),
        hardware.base_freq_mhz.map(|value| format!("{value} MHz")),
    );

    let breakdown = hardware.core_breakdown;
    if breakdown.total() > 0 {
        push_value(
            &mut rows,
            t("system.field.core_layout"),
            Some(format!(
                "P {} · E {} · LP-E {}",
                breakdown.p_cores, breakdown.e_cores, breakdown.lp_cores
            )),
        );
    }
    // Instruction-set features from the native CPU source. The row is absent
    // when no source reported features — an honest skip, never a dash.
    if !hardware.instruction_features.is_empty() {
        let labels: Vec<&str> = hardware
            .instruction_features
            .iter()
            .map(|feature| feature.label())
            .collect();
        push_value(
            &mut rows,
            t("system.field.instruction_features"),
            Some(labels.join(" · ")),
        );
    }

    push_text(
        &mut rows,
        t("system.field.virtualization"),
        hardware.virt.as_deref(),
    );
    push_text(
        &mut rows,
        t("system.field.product"),
        hardware.product_name.as_deref(),
    );
    push_text(
        &mut rows,
        t("system.field.product_version"),
        hardware.product_version.as_deref(),
    );
    push_text(
        &mut rows,
        t("system.field.firmware_vendor"),
        hardware.firmware_vendor.as_deref(),
    );
    push_text(
        &mut rows,
        t("system.field.firmware_version"),
        hardware.firmware_version.as_deref(),
    );
    push_text(
        &mut rows,
        t("system.field.architecture"),
        hardware.architecture.as_deref(),
    );
    push_text(
        &mut rows,
        t("system.field.motherboard_vendor"),
        hardware.motherboard_vendor.as_deref(),
    );
    push_text(
        &mut rows,
        t("system.field.motherboard_model"),
        hardware.motherboard_model.as_deref(),
    );
    push_text(
        &mut rows,
        t("system.field.firmware_release_date"),
        hardware.firmware_release_date.as_deref(),
    );
    if let Some(secure_boot) = hardware.secure_boot {
        rows.push(SystemInfoRow {
            label: t("system.secure_boot").to_string(),
            value: if secure_boot {
                t("common.enabled").to_string()
            } else {
                t("common.disabled").to_string()
            },
        });
    }
    rows
}

fn display_summary(display: &DisplayInfo) -> String {
    let identity = [display.manufacturer.as_deref(), display.model.as_deref()]
        .into_iter()
        .flatten()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let mut parts = vec![display.connector.clone()];
    if !identity.is_empty() {
        parts.push(identity);
    }
    if let (Some(width), Some(height)) = (display.width_px, display.height_px) {
        parts.push(format!("{width}×{height}"));
    }
    if let Some(refresh) = display
        .refresh_hz
        .filter(|value| value.is_finite() && *value > 0.0)
    {
        parts.push(format!("{refresh:.1} Hz"));
    }
    if let Some(hdr) = display_hdr_capability(display) {
        parts.push(hdr);
    }
    if let (Some(width), Some(height)) = (display.width_mm, display.height_mm) {
        parts.push(format!("{width}×{height} mm"));
    }
    if let Some(serial) = display.serial.as_deref().filter(|value| !value.is_empty()) {
        parts.push(format!("S/N {serial}"));
    }
    parts.join(" · ")
}

fn display_hdr_capability(display: &DisplayInfo) -> Option<String> {
    let state = match display.hdr_supported {
        Some(true) => t("system.hdr_supported"),
        Some(false) => t("system.hdr_unsupported"),
        None => return None,
    };
    Some(format!("{} {state}", t("system.hdr")))
}

pub(super) fn telemetry_rows(snapshot: &SystemSnapshot) -> Vec<SystemInfoRow> {
    vec![
        SystemInfoRow {
            label: t("common.uptime").to_string(),
            value: duration(snapshot.uptime_secs),
        },
        SystemInfoRow {
            label: t("common.processes").to_string(),
            value: snapshot.processes.to_string(),
        },
        SystemInfoRow {
            label: t("common.threads").to_string(),
            value: snapshot
                .threads
                .map_or_else(missing_value, |threads| threads.to_string()),
        },
    ]
}

fn push_text(rows: &mut Vec<SystemInfoRow>, label: &str, value: Option<&str>) {
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        rows.push(SystemInfoRow {
            label: label.into(),
            value: value.into(),
        });
    }
}

fn push_value<T: ToString>(rows: &mut Vec<SystemInfoRow>, label: &str, value: Option<T>) {
    if let Some(value) = value {
        rows.push(SystemInfoRow {
            label: label.into(),
            value: value.to_string(),
        });
    }
}
