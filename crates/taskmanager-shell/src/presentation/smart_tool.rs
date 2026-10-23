//! The remediation a frontend shows for a missing SMART tool.
//!
//! The shared fact owns the probe command and the guidance paragraph; the
//! platform layer refines it with its own install command. A renderer copies
//! the command from this projection instead of composing a command of its
//! own.

use taskmanager_core::core::metrics::DiskMetrics;

/// The copyable remediation for one device whose SMART tool is missing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmartToolGuidance {
    /// The localization key of the headline.
    pub title_key: &'static str,
    /// The localization key of the install guidance paragraph.
    pub guidance_key: &'static str,
    /// The command the user copies to verify the tool and list the
    /// SMART-capable devices.
    pub probe_command: String,
    /// The platform's concrete install command, when the platform knows one.
    pub install_command: Option<String>,
}

/// The remediation for one disk, or `None` while its SMART tool is present.
///
/// The probe command and the guidance keys come from the shared fact; the
/// install command is the platform's own and stays absent where the platform
/// cannot name one.
#[must_use]
pub fn smart_tool_guidance(disk: &DiskMetrics) -> Option<SmartToolGuidance> {
    let remediation = disk.smart_tool_remediation_hint()?;
    Some(SmartToolGuidance {
        title_key: "disk.smart_missing_tool",
        guidance_key: remediation.guidance.i18n_key(),
        probe_command: remediation.probe_command,
        install_command: remediation.install_command,
    })
}

#[cfg(test)]
#[path = "../../tests/headless/presentation_smart_tool_tests.rs"]
mod tests;
