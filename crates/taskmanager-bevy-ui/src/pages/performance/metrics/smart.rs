//! Actionable SMART source state remains separate from older measurements.
use taskmanager_application::i18n::t;
use taskmanager_core::core::metrics::{DiskMetrics, SmartAvailability};
use taskmanager_shell::presentation::{device_status_i18n_key, effective_smart_status};

pub(in crate::pages::performance) fn smart_source_status(disk: &DiskMetrics) -> String {
    if matches!(
        disk.smart_availability,
        SmartAvailability::MissingTool | SmartAvailability::PermissionDenied
    ) {
        format!(
            "{}: {}",
            t("disk.smart_status"),
            t(device_status_i18n_key(effective_smart_status(disk)))
        )
    } else {
        String::new()
    }
}

pub(in crate::pages::performance) fn smart_source_guidance(disk: &DiskMetrics) -> String {
    match disk.smart_availability {
        SmartAvailability::MissingTool => t("device.action_missing_tool").to_owned(),
        SmartAvailability::PermissionDenied => t("device.action_permission").to_owned(),
        _ => String::new(),
    }
}
