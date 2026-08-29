//! Iced About / system-information modal: the hardware facts and version
//! line, rendered from the shared `HardwareInfo` + `SystemSnapshot` (the same
//! data the TUI and GPUI surfaces show).

use iced::widget::{column, row, scrollable, text};
use iced::{Element, Length};
use taskmanager_application::i18n::t;
use taskmanager_assets::product;
use taskmanager_core::core::hardware::HardwareInfo;
use taskmanager_core::core::metrics::SystemSnapshot;
use taskmanager_theme::tokens;

use crate::app::Message;
use crate::i18n::{self, Key};

use super::overlays::modal_overlay;
use super::system_table::SystemInfoRow;
use taskmanager_shell::presentation::{duration, missing_value};

/// Render the about modal for the current shell state. Every value comes
/// from the shared data domains; unavailable facts simply render `—`.
pub(super) fn render(app: &crate::IcedApp) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let appear = app.modal_appear_progress();
    let theme_snapshot = app.theme();
    let language = app.language();
    let shell = &app.shell;

    let rows = about_rows(
        shell.projection().hardware.as_ref(),
        shell.projection().snapshot.as_ref(),
    );
    let version = format!("{} {}", product::ICED_NAME, env!("CARGO_PKG_VERSION"));

    let body: Vec<Element<'_, Message, iced::Theme, iced::Renderer>> = rows
        .into_iter()
        .map(|row| {
            row![
                text(row.label.clone()).width(Length::Fixed(170.0)),
                text(row.value.clone()).width(Length::Fill),
            ]
            .spacing(8)
            .padding(4)
            .width(Length::Fill)
            .into()
        })
        .collect();

    modal_overlay(
        theme_snapshot,
        i18n::t(language, Key::SystemInfo),
        "Hardware facts + live snapshot · Esc closes",
        column![
            text(version).size(f32::from(tokens::FONT_14)),
            scrollable(column(body).spacing(1))
                .height(Length::Fixed(320.0))
                .width(Length::Fill),
            row![
                crate::focus::ghost_button(
                    theme_snapshot,
                    crate::app::FocusTarget::AboutCopyDetails,
                    t("about.copy_details"),
                    Message::CopyAboutDetails,
                ),
                crate::focus::ghost_button(
                    theme_snapshot,
                    crate::app::FocusTarget::Export,
                    t("common.export"),
                    Message::GenerateDiagnosticsReport,
                ),
            ]
            .spacing(8),
        ]
        .spacing(8)
        .into(),
        appear,
    )
}

/// Build the clipboard payload for the About modal's copy-details action
/// (G-16): the same rows the modal renders, prefixed by the version line —
/// never a second, drifting source of the facts.
#[must_use]
pub(crate) fn about_copy_payload(
    hardware: Option<&HardwareInfo>,
    snapshot: Option<&SystemSnapshot>,
) -> String {
    let rows = about_rows(hardware, snapshot);
    let version = format!("{} {}", product::ICED_NAME, env!("CARGO_PKG_VERSION"));
    let mut payload = version;
    for row in &rows {
        payload.push('\n');
        payload.push_str(&row.label);
        payload.push_str(": ");
        payload.push_str(&row.value);
    }
    payload
}

/// The about kv rows: hostname/os/kernel/cpu/cores/memory/uptime. Rows with
/// no value render `—` rather than disappearing, matching the TUI's static
/// about list shape.
#[must_use]
pub(super) fn about_rows(
    hardware: Option<&HardwareInfo>,
    snapshot: Option<&SystemSnapshot>,
) -> Vec<SystemInfoRow> {
    let mut rows = Vec::new();
    // Field labels resolve through the same catalog keys the System page
    // hardware block reads (single source — the About modal and the System
    // facts never drift apart under a second language).
    push(
        &mut rows,
        t("system.hostname"),
        hardware.and_then(|h| h.hostname.as_deref()),
    );
    push(
        &mut rows,
        t("system.field.os_name"),
        hardware.and_then(|h| h.os_name.as_deref()),
    );
    push(
        &mut rows,
        t("system.field.os_version"),
        hardware.and_then(|h| h.os_version.as_deref()),
    );
    push(
        &mut rows,
        t("system.kernel"),
        hardware.and_then(|h| h.kernel_version.as_deref()),
    );
    push(
        &mut rows,
        t("system.field.architecture"),
        hardware.and_then(|h| h.architecture.as_deref()),
    );
    push(
        &mut rows,
        t("system.field.motherboard_vendor"),
        hardware.and_then(|h| h.motherboard_vendor.as_deref()),
    );
    push(
        &mut rows,
        t("system.field.motherboard_model"),
        hardware.and_then(|h| h.motherboard_model.as_deref()),
    );
    push(
        &mut rows,
        t("system.field.firmware_release_date"),
        hardware.and_then(|h| h.firmware_release_date.as_deref()),
    );
    if let Some(secure_boot) = hardware.and_then(|h| h.secure_boot) {
        rows.push(SystemInfoRow {
            label: t("system.secure_boot").to_string(),
            value: if secure_boot {
                t("common.enabled").to_string()
            } else {
                t("common.disabled").to_string()
            },
        });
    }
    push(
        &mut rows,
        t("common.cpu"),
        hardware.and_then(|h| h.cpu_brand.as_deref()),
    );
    push_value(
        &mut rows,
        t("common.logical_cores"),
        hardware
            .and_then(|h| h.cpu_cores)
            .map(|cores| cores.to_string()),
    );
    push_value(
        &mut rows,
        t("system.field.installed_memory"),
        hardware
            .and_then(|h| h.total_memory_mb)
            .map(|mib| format!("{mib} MiB")),
    );
    push_value(
        &mut rows,
        t("common.uptime"),
        snapshot.map(|s| duration(s.uptime_secs)),
    );
    rows
}

fn push(rows: &mut Vec<SystemInfoRow>, label: &str, value: Option<&str>) {
    rows.push(SystemInfoRow {
        label: label.into(),
        value: value.map_or_else(missing_value, str::to_owned),
    });
}

fn push_value(rows: &mut Vec<SystemInfoRow>, label: &str, value: Option<String>) {
    rows.push(SystemInfoRow {
        label: label.into(),
        value: value.unwrap_or_else(missing_value),
    });
}

#[cfg(test)]
#[path = "../../tests/gui/ui/about_tests.rs"]
mod tests;
