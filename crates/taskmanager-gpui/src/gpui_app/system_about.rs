//! Mission Center-compatible System Information dialog.
//!
//! This surface is deliberately a projection only: it consumes the already
//! correlated `HardwareInfo` and desktop-appearance read models. It does not
//! read `/proc`, run commands, or invent package-manager/desktop facts that the
//! active provider did not publish.

use gpui::{
    App, ClipboardItem, Context, Div, Entity, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::core::{DesktopAppearance, DesktopFamily, HardwareInfo, PreferredColorScheme};
use crate::gpui_app::elements;
use crate::gpui_app::root::RootView;
use crate::gpui_app::theme::{Theme, tokens};
use crate::i18n;
use taskmanager_ui::primitives::selectable_text::SelectableText;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemAboutRow {
    pub label_key: &'static str,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemAboutGroup {
    pub title_key: &'static str,
    pub rows: Vec<SystemAboutRow>,
}

fn optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn family_text(family: DesktopFamily) -> Option<String> {
    match family {
        DesktopFamily::Gnome => Some("GNOME".to_owned()),
        DesktopFamily::Kde => Some("KDE Plasma".to_owned()),
        DesktopFamily::Windows => Some("Windows".to_owned()),
        DesktopFamily::Macos => Some("macOS".to_owned()),
        DesktopFamily::Unknown => None,
    }
}

fn scheme_text(scheme: PreferredColorScheme) -> Option<String> {
    match scheme {
        PreferredColorScheme::Light => Some(i18n::t("system_about.light").to_string()),
        PreferredColorScheme::Dark => Some(i18n::t("system_about.dark").to_string()),
        PreferredColorScheme::Unknown => None,
    }
}

fn push_optional(rows: &mut Vec<SystemAboutRow>, label_key: &'static str, value: Option<String>) {
    if let Some(value) = value {
        rows.push(SystemAboutRow { label_key, value });
    }
}

/// Project the facts already held by the frontend into the dialog's grouped
/// rows. Absence removes a row, matching Mission Center's conditional system
/// information groups; measured zero remains a real string when supplied.
#[must_use]
pub fn groups(hardware: &HardwareInfo, appearance: DesktopAppearance) -> Vec<SystemAboutGroup> {
    let mut operating_system = Vec::new();
    push_optional(
        &mut operating_system,
        "system_about.name",
        optional_text(hardware.os_name.as_deref()),
    );
    push_optional(
        &mut operating_system,
        "system_about.version",
        optional_text(hardware.os_version.as_deref()),
    );
    push_optional(
        &mut operating_system,
        "system_about.package_manager",
        optional_text(hardware.package_manager.as_deref()),
    );
    push_optional(
        &mut operating_system,
        "system_about.package_manager_version",
        optional_text(hardware.package_manager_version.as_deref()),
    );
    push_optional(
        &mut operating_system,
        "system_about.package_count",
        hardware.package_count.map(|count| count.to_string()),
    );
    push_optional(
        &mut operating_system,
        "system_about.hostname",
        optional_text(hardware.hostname.as_deref()),
    );
    push_optional(
        &mut operating_system,
        "system_about.shell",
        optional_text(hardware.shell.as_deref()),
    );
    push_optional(
        &mut operating_system,
        "system_about.locale",
        optional_text(hardware.locale.as_deref()),
    );
    push_optional(
        &mut operating_system,
        "system_about.init_system",
        optional_text(hardware.init_system.as_deref()),
    );

    let mut kernel = Vec::new();
    push_optional(
        &mut kernel,
        "system_about.release",
        optional_text(hardware.kernel_version.as_deref()),
    );
    push_optional(
        &mut kernel,
        "system_about.version",
        optional_text(hardware.kernel_build.as_deref()),
    );

    let mut desktop = Vec::new();
    push_optional(
        &mut desktop,
        "system_about.name",
        family_text(appearance.family)
            .or_else(|| optional_text(hardware.desktop_environment.as_deref())),
    );
    push_optional(
        &mut desktop,
        "system_about.version",
        optional_text(hardware.desktop_environment_version.as_deref()),
    );
    push_optional(
        &mut desktop,
        "system_about.windowing_system",
        optional_text(hardware.windowing_system.as_deref()),
    );
    let window_manager = optional_text(hardware.window_manager.as_deref()).map(|name| {
        optional_text(hardware.window_manager_version.as_deref())
            .map_or(name.clone(), |version| format!("{name} {version}"))
    });
    push_optional(&mut desktop, "system_about.window_manager", window_manager);
    push_optional(
        &mut desktop,
        "system_about.compositor_backend",
        optional_text(hardware.compositor_backend.as_deref()),
    );
    push_optional(
        &mut desktop,
        "system_about.virtual_terminal",
        optional_text(hardware.virtual_terminal.as_deref()),
    );
    push_optional(
        &mut desktop,
        "system_about.terminal",
        optional_text(hardware.terminal.as_deref()),
    );
    push_optional(
        &mut desktop,
        "system_about.terminal_version",
        optional_text(hardware.terminal_version.as_deref()),
    );
    push_optional(
        &mut desktop,
        "system_about.color_scheme",
        scheme_text(appearance.color_scheme),
    );

    let mut hardware_rows = Vec::new();
    push_optional(
        &mut hardware_rows,
        "system_about.cpu",
        optional_text(hardware.cpu_brand.as_deref()),
    );
    let cores = match (hardware.core_breakdown.total(), hardware.cpu_cores) {
        (physical, Some(logical)) if physical > 0 => Some(format!("{physical} / {logical}")),
        (0, Some(logical)) => Some(logical.to_string()),
        (physical, None) if physical > 0 => Some(physical.to_string()),
        _ => None,
    };
    push_optional(&mut hardware_rows, "system_about.cores", cores);
    push_optional(
        &mut hardware_rows,
        "system_about.memory",
        hardware.total_memory_mb.map(format_memory),
    );
    push_optional(
        &mut hardware_rows,
        "system_about.virtualization",
        optional_text(hardware.virt.as_deref()),
    );
    for display in &hardware.displays {
        push_optional(
            &mut hardware_rows,
            "system.display",
            display_summary(display),
        );
    }

    [
        ("system_about.operating_system", operating_system),
        ("system_about.kernel", kernel),
        ("system_about.desktop", desktop),
        ("system_about.hardware", hardware_rows),
    ]
    .into_iter()
    .filter(|(_, rows)| !rows.is_empty())
    .map(|(title_key, rows)| SystemAboutGroup { title_key, rows })
    .collect()
}

fn format_memory(total_memory_mb: u64) -> String {
    if total_memory_mb >= 1024 {
        format!("{:.1} GiB", total_memory_mb as f64 / 1024.0)
    } else {
        format!("{total_memory_mb} MiB")
    }
}

fn display_summary(display: &crate::core::hardware::DisplayInfo) -> Option<String> {
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
    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn display_hdr_capability(display: &crate::core::hardware::DisplayInfo) -> Option<String> {
    let state = match display.hdr_supported {
        Some(true) => i18n::t("system.hdr_supported"),
        Some(false) => i18n::t("system.hdr_unsupported"),
        None => return None,
    };
    Some(format!("{} {state}", i18n::t("system.hdr")))
}

/// Keep the visible prefix of provider-owned values readable in the fixed
/// two-column dialog rows. The complete value remains in the copy action; this
/// projection only prevents right-aligned overflow from hiding the useful
/// beginning of a long kernel/CPU descriptor.
fn display_value(value: &str) -> String {
    const MAX_CHARS: usize = 36;
    if value.chars().count() <= MAX_CHARS {
        return value.to_owned();
    }
    let mut displayed: String = value.chars().take(MAX_CHARS - 1).collect();
    displayed.push('…');
    displayed
}

/// Stable plain-text representation used by the dialog's Copy All action.
#[must_use]
pub fn copy_all_text(groups: &[SystemAboutGroup]) -> String {
    let mut text = i18n::t("system_about.title").to_string();
    for group in groups {
        text.push_str("\n\n");
        text.push_str(i18n::t(group.title_key));
        for row in &group.rows {
            text.push('\n');
            text.push_str(i18n::t(row.label_key));
            text.push_str(": ");
            text.push_str(&row.value);
        }
    }
    text
}

fn render_row(
    theme: &Theme,
    row: &SystemAboutRow,
    index: usize,
    cx: &mut Context<RootView>,
) -> impl IntoElement {
    let value = row.value.clone();
    let display_value = display_value(&value);
    let copy_value = format!("{}: {}", i18n::t(row.label_key), value);
    div()
        .id(("system-about-row", index))
        .debug_selector(move || format!("system-about-row-{index}"))
        .focusable()
        .tab_stop(true)
        .focus(elements::focus_ring(theme))
        .cursor_pointer()
        .on_click(cx.listener(move |_view, _event, _window, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(copy_value.clone()));
        }))
        .px(tokens::SPACE_10)
        .py(tokens::SPACE_8)
        .flex()
        .items_center()
        .justify_between()
        .gap(tokens::SPACE_12)
        .child(
            elements::truncated_text(i18n::t(row.label_key))
                .flex_1()
                .min_w(px(0.0))
                .text_size(tokens::FONT_13)
                .text_color(theme.fg),
        )
        .child(
            div()
                .debug_selector(move || format!("system-about-value-{index}"))
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .text_right()
                .text_size(tokens::FONT_13)
                .text_color(theme.fg_dim)
                .child(
                    SelectableText::new(
                        ("system-about-selectable-value", index),
                        display_value,
                        theme.palette(),
                    )
                    .debug_selector(format!("system-about-selectable-value-{index}")),
                ),
        )
}

pub struct SystemAboutView {
    pub actions: Div,
    pub groups: Div,
}

pub fn render_system_about(
    theme: &Theme,
    hardware: &HardwareInfo,
    appearance: DesktopAppearance,
    entity: Entity<RootView>,
    cx: &mut Context<RootView>,
) -> SystemAboutView {
    let groups = groups(hardware, appearance);
    let copy_text = copy_all_text(&groups);
    let mut content = div().flex().flex_col().gap(tokens::SPACE_12);
    let mut row_index = 0;
    for (group_index, group) in groups.iter().enumerate() {
        let mut rows = div().flex().flex_col();
        for row in &group.rows {
            rows = rows.child(render_row(theme, row, row_index, cx));
            row_index += 1;
        }
        content = content.child(
            div()
                .flex()
                .flex_col()
                .gap(tokens::SPACE_4)
                .child(
                    div()
                        .debug_selector(move || {
                            format!("tm-system-about-section-title-{group_index}")
                        })
                        .pl(tokens::SPACE_2)
                        .text_size(tokens::FONT_12)
                        .text_color(theme.fg_dim)
                        .child(i18n::t(group.title_key)),
                )
                .child(
                    div()
                        .debug_selector(move || {
                            format!("tm-system-about-section-card-{group_index}")
                        })
                        .rounded(tokens::card_radius(theme))
                        .bg(theme.sidebar_card_bg)
                        .overflow_hidden()
                        .child(rows),
                ),
        );
    }
    if groups.is_empty() {
        content = content.child(
            div()
                .text_size(tokens::FONT_13)
                .text_color(theme.fg_dim)
                .child(i18n::t("system_about.unavailable")),
        );
    }

    let copy_entity = entity;
    let copy = div()
        .debug_selector(|| "tm-system-about-copy".to_string())
        .flex_none()
        .child(elements::pill(
            theme,
            "system-about-copy-all",
            i18n::t("system_about.copy_all"),
            true,
            false,
            move |_window, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(copy_text.clone()));
                copy_entity.update(cx, |_view, cx| cx.notify());
            },
            |_hovered: &bool, _window: &mut Window, _cx: &mut App| {},
        ));
    let actions = div()
        .debug_selector(|| "tm-system-about-actions".to_string())
        .flex()
        .flex_row()
        .items_center()
        .justify_start()
        .child(copy);
    SystemAboutView {
        actions,
        groups: content,
    }
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_system_about_tests.rs"]
mod tests;
