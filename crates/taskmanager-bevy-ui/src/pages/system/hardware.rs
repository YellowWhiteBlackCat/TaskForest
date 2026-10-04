//! Static hardware facts independent from on-demand memory and NPU lanes.

use taskmanager_application::i18n::t;
use taskmanager_core::core::hardware::HardwareInfo;
use taskmanager_shell::presentation::{kernel_error_summary, missing_value};

use super::{SystemFactGroup, SystemFactRow, clean_memory_size, joined, optional};

pub(super) fn hardware_fact_rows(hardware: &HardwareInfo) -> Vec<SystemFactRow> {
    let mut rows: Vec<SystemFactRow> = Vec::new();
    let dash = missing_value();
    rows.push(SystemFactRow {
        group: SystemFactGroup::OperatingSystem,
        label: t("system.hostname").to_owned(),
        value: optional(hardware.hostname.as_deref()),
    });
    rows.push(SystemFactRow {
        group: SystemFactGroup::OperatingSystem,
        label: t("system.os").to_owned(),
        value: joined(hardware.os_name.as_deref(), hardware.os_version.as_deref()),
    });
    rows.push(SystemFactRow {
        group: SystemFactGroup::OperatingSystem,
        label: t("system.kernel").to_owned(),
        value: joined(
            hardware.kernel_version.as_deref(),
            hardware.kernel_build.as_deref(),
        ),
    });
    if let Some(errors) = kernel_error_summary(hardware) {
        rows.push(SystemFactRow {
            group: SystemFactGroup::OperatingSystem,
            label: t("system.kernel_errors").to_owned(),
            value: errors,
        });
    }
    if let Some(count) = hardware.kernel_modules_count {
        rows.push(SystemFactRow {
            group: SystemFactGroup::OperatingSystem,
            label: t("system.kernel_modules").to_owned(),
            value: count.to_string(),
        });
    }
    rows.push(SystemFactRow {
        group: SystemFactGroup::Hardware,
        label: t("system.model").to_owned(),
        value: joined(
            hardware.product_name.as_deref(),
            hardware.product_version.as_deref(),
        ),
    });
    rows.push(SystemFactRow {
        group: SystemFactGroup::Hardware,
        label: t("system.firmware").to_owned(),
        value: optional(hardware.firmware_vendor.as_deref()),
    });
    rows.push(SystemFactRow {
        group: SystemFactGroup::Cpu,
        label: t("system.field.cpu").to_owned(),
        value: optional(hardware.cpu_brand.as_deref()),
    });
    let cores = hardware
        .cpu_cores
        .map_or_else(|| dash.clone(), |cores| cores.to_string());
    rows.push(SystemFactRow {
        group: SystemFactGroup::Cpu,
        label: t("system.field.cores").to_owned(),
        value: cores,
    });
    if let Some(memory) = hardware.total_memory_mb {
        rows.push(SystemFactRow {
            group: SystemFactGroup::Memory,
            label: t("system.section.memory").to_owned(),
            value: clean_memory_size(memory),
        });
    }
    if let Some(virt) = hardware.virt.as_deref() {
        rows.push(SystemFactRow {
            group: SystemFactGroup::OperatingSystem,
            label: t("system.field.virt").to_owned(),
            value: virt.to_owned(),
        });
    }
    rows.push(SystemFactRow {
        group: SystemFactGroup::OperatingSystem,
        label: t("system.desktop_environment").to_owned(),
        value: joined(
            hardware.desktop_environment.as_deref(),
            hardware.desktop_environment_version.as_deref(),
        ),
    });
    rows.push(SystemFactRow {
        group: SystemFactGroup::OperatingSystem,
        label: t("system.windowing_system").to_owned(),
        value: joined(
            hardware.windowing_system.as_deref(),
            hardware.window_manager.as_deref(),
        ),
    });
    rows.push(SystemFactRow {
        group: SystemFactGroup::OperatingSystem,
        label: t("system.field.init_system").to_owned(),
        value: optional(hardware.init_system.as_deref()),
    });
    if let Some(manager) = hardware.package_manager.as_deref() {
        let value = hardware.package_manager_version.as_deref().map_or_else(
            || manager.to_owned(),
            |version| format!("{manager} {version}"),
        );
        rows.push(SystemFactRow {
            group: SystemFactGroup::OperatingSystem,
            label: t("system.package_manager").to_owned(),
            value,
        });
    }
    rows.push(SystemFactRow {
        group: SystemFactGroup::OperatingSystem,
        label: t("system.field.shell").to_owned(),
        value: optional(hardware.shell.as_deref()),
    });
    rows.push(SystemFactRow {
        group: SystemFactGroup::OperatingSystem,
        label: t("system.field.locale").to_owned(),
        value: optional(hardware.locale.as_deref()),
    });
    rows
}
