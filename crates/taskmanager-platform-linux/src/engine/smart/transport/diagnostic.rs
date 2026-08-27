//! Structured smartctl command diagnostics.
//!
//! JSON-mode smartctl writes invocation errors to
//! `/smartctl/messages`, frequently on stdout. Only that structured channel
//! and a bounded set of known stderr messages may authorize a device-type
//! fallback; arbitrary stdout text is never command-plan authority.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SmartctlCommandDiagnostic {
    PermissionDenied,
    DeviceUnavailable,
    CommandFailure,
    DeviceTypeRequired,
    Unsupported,
}

pub(super) fn classify_smartctl_command_diagnostic(
    stdout: &[u8],
    stderr: &[u8],
) -> Option<SmartctlCommandDiagnostic> {
    let mut diagnostics = smartctl_json_messages(stdout)
        .into_iter()
        .filter_map(|message| classify_smartctl_message(&message))
        .collect::<Vec<_>>();
    diagnostics.extend(classify_smartctl_stderr(stderr));
    diagnostics
        .into_iter()
        .max_by_key(|diagnostic| smartctl_diagnostic_priority(*diagnostic))
        .or_else(|| {
            smartctl_json_has_error_message(stdout)
                .then_some(SmartctlCommandDiagnostic::CommandFailure)
        })
}

pub(super) fn smartctl_json_messages(stdout: &[u8]) -> Vec<String> {
    let Ok(root) = serde_json::from_slice::<serde_json::Value>(stdout) else {
        return Vec::new();
    };
    root.pointer("/smartctl/messages")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|message| message.get("string").and_then(serde_json::Value::as_str))
        .map(ToOwned::to_owned)
        .collect()
}

fn smartctl_json_has_error_message(stdout: &[u8]) -> bool {
    let Ok(root) = serde_json::from_slice::<serde_json::Value>(stdout) else {
        return false;
    };
    root.pointer("/smartctl/messages")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|messages| {
            messages.iter().any(|message| {
                message
                    .get("severity")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|severity| severity.eq_ignore_ascii_case("error"))
            })
        })
}

fn classify_smartctl_stderr(stderr: &[u8]) -> Vec<SmartctlCommandDiagnostic> {
    let message = String::from_utf8_lossy(stderr);
    classify_smartctl_message(&message).into_iter().collect()
}

fn classify_smartctl_message(message: &str) -> Option<SmartctlCommandDiagnostic> {
    let message = message.to_ascii_lowercase();
    if [
        "permission denied",
        "operation not permitted",
        "insufficient privileges",
    ]
    .iter()
    .any(|needle| message.contains(needle))
    {
        Some(SmartctlCommandDiagnostic::PermissionDenied)
    } else if [
        "no such device",
        "no such file or directory",
        "device not found",
        "device has disappeared",
    ]
    .iter()
    .any(|needle| message.contains(needle))
    {
        Some(SmartctlCommandDiagnostic::DeviceUnavailable)
    } else if [
        "please specify device type",
        "requires option '-d",
        "try adding '-d",
        "unknown usb bridge",
        "unsupported usb bridge",
        "device type mismatch",
    ]
    .iter()
    .any(|needle| message.contains(needle))
    {
        Some(SmartctlCommandDiagnostic::DeviceTypeRequired)
    } else if [
        "smart support is: unavailable",
        "device lacks smart capability",
        "smart is not supported",
        "smart unsupported",
    ]
    .iter()
    .any(|needle| message.contains(needle))
    {
        Some(SmartctlCommandDiagnostic::Unsupported)
    } else {
        None
    }
}

const fn smartctl_diagnostic_priority(diagnostic: SmartctlCommandDiagnostic) -> u8 {
    match diagnostic {
        SmartctlCommandDiagnostic::PermissionDenied => 5,
        SmartctlCommandDiagnostic::DeviceUnavailable => 4,
        SmartctlCommandDiagnostic::CommandFailure => 3,
        SmartctlCommandDiagnostic::DeviceTypeRequired => 2,
        SmartctlCommandDiagnostic::Unsupported => 1,
    }
}

pub(super) fn smartctl_json_reports_unsupported(stdout: &str) -> bool {
    let Ok(root) = serde_json::from_str::<serde_json::Value>(stdout) else {
        return false;
    };
    root.pointer("/smart_support/available")
        .and_then(serde_json::Value::as_bool)
        == Some(false)
        || smartctl_json_messages(stdout.as_bytes())
            .iter()
            .any(|message| {
                classify_smartctl_message(message) == Some(SmartctlCommandDiagnostic::Unsupported)
            })
}

pub(in crate::engine::smart) fn command_output_is_permission_denied(
    stdout: &[u8],
    stderr: &[u8],
) -> bool {
    classify_smartctl_command_diagnostic(stdout, stderr)
        == Some(SmartctlCommandDiagnostic::PermissionDenied)
}

pub(in crate::engine::smart) fn command_output_requests_device_type(
    stdout: &[u8],
    stderr: &[u8],
) -> bool {
    classify_smartctl_command_diagnostic(stdout, stderr)
        == Some(SmartctlCommandDiagnostic::DeviceTypeRequired)
}
