//! Localized explanations for the canonical service exit-status vocabulary.

use taskmanager_application::i18n;
use taskmanager_core::core::services::service_exit_status_label;

pub(super) fn exit_status_text(status: i32) -> String {
    let key = match status {
        64 => "svc.exit_usage",
        65 => "svc.exit_data",
        66 => "svc.exit_input",
        67 => "svc.exit_user",
        68 => "svc.exit_host",
        69 => "svc.exit_unavailable",
        70 => "svc.exit_software",
        71 => "svc.exit_os",
        72 => "svc.exit_os_file",
        73 => "svc.exit_create",
        74 => "svc.exit_io",
        75 => "svc.exit_temporary",
        76 => "svc.exit_protocol",
        77 => "svc.exit_permission",
        78 => "svc.exit_configuration",
        _ => return status.to_string(),
    };
    match service_exit_status_label(status) {
        Some(label) => format!("{status} ({label}) · {}", i18n::t(key)),
        None => status.to_string(),
    }
}
