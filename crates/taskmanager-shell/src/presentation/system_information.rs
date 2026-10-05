//! Toolkit-neutral system-information groups from cached provider facts.

use taskmanager_application::i18n;
use taskmanager_core::core::appearance::{DesktopAppearance, DesktopFamily, PreferredColorScheme};
use taskmanager_core::core::hardware::{DisplayInfo, HardwareInfo};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemInformationRow {
    pub label_key: &'static str,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemInformationGroup {
    pub title_key: &'static str,
    pub rows: Vec<SystemInformationRow>,
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

fn push_optional(
    rows: &mut Vec<SystemInformationRow>,
    label_key: &'static str,
    value: Option<String>,
) {
    if let Some(value) = value {
        rows.push(SystemInformationRow { label_key, value });
    }
}

/// Project the facts already held by the frontend into the dialog's grouped
/// rows. Absence removes a row, matching Mission Center's conditional system
/// information groups; measured zero remains a real string when supplied.
#[must_use]
pub fn groups(
    hardware: &HardwareInfo,
    appearance: DesktopAppearance,
) -> Vec<SystemInformationGroup> {
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
    push_optional(
        &mut kernel,
        "system_about.compiler",
        optional_text(hardware.kernel_compiler.as_deref()),
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
    .map(|(title_key, rows)| SystemInformationGroup { title_key, rows })
    .collect()
}

fn format_memory(total_memory_mb: u64) -> String {
    if total_memory_mb >= 1024 {
        format!("{:.1} GiB", total_memory_mb as f64 / 1024.0)
    } else {
        format!("{total_memory_mb} MiB")
    }
}

fn display_summary(display: &DisplayInfo) -> Option<String> {
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

fn display_hdr_capability(display: &DisplayInfo) -> Option<String> {
    let state = match display.hdr_supported {
        Some(true) => i18n::t("system.hdr_supported"),
        Some(false) => i18n::t("system.hdr_unsupported"),
        None => return None,
    };
    Some(format!("{} {state}", i18n::t("system.hdr")))
}

/// Stable plain-text representation used by the dialog's Copy All action.
#[must_use]
pub fn copy_all_text(groups: &[SystemInformationGroup]) -> String {
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

#[cfg(test)]
#[path = "../../tests/headless/presentation/system_information_tests.rs"]
mod tests;
