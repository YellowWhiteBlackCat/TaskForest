//! The Linux remediation for a missing SMART tool.
//!
//! The probe command and the guidance paragraph belong to the shared
//! contract; the install command belongs to the platform layer. Linux
//! distributions ship `smartmontools` under different package managers, so
//! this platform names the install command only where it can see the manager
//! itself — an unknown distribution leaves the install command absent instead
//! of inventing a package manager.

use std::path::Path;

use taskforest_core::core::metrics::{SmartAvailability, SmartToolRemediation};

/// The package managers this platform adapter knows how to install
/// `smartmontools` with, in detection order. Each entry pairs the manager's
/// own binary path with the command that installs the package.
const INSTALL_COMMANDS: [(&str, &str); 5] = [
    ("bin/apt-get", "sudo apt install smartmontools"),
    ("bin/dnf", "sudo dnf install smartmontools"),
    ("bin/pacman", "sudo pacman -S smartmontools"),
    ("bin/zypper", "sudo zypper install smartmontools"),
    ("bin/apk", "sudo apk add smartmontools"),
];

/// The install command of the package manager present under `root`.
///
/// `None` when no known manager is installed: the remediation then carries
/// the probe command only.
#[must_use]
pub(crate) fn install_command(root: &Path) -> Option<&'static str> {
    INSTALL_COMMANDS
        .iter()
        .find(|(relative, _)| root.join(relative).exists())
        .map(|(_, command)| *command)
}

/// The install command of this host's package manager.
#[must_use]
pub(crate) fn host_install_command() -> Option<&'static str> {
    install_command(Path::new("/"))
}

/// The remediation for a device whose SMART tool is missing.
///
/// `None` while the tool is present. The platform-neutral remediation of the
/// contract is the baseline; a host with a known package manager refines it
/// with that manager's install command.
#[must_use]
pub(crate) fn remediation(
    availability: SmartAvailability,
    install_command: Option<&'static str>,
) -> Option<SmartToolRemediation> {
    if availability != SmartAvailability::MissingTool {
        return None;
    }
    let remediation = SmartToolRemediation::platform_neutral();
    Some(match install_command {
        Some(command) => remediation.with_install_command(command),
        None => remediation,
    })
}

#[cfg(test)]
#[path = "../../../tests/headless/engine/smart/tool_tests.rs"]
mod tests;
