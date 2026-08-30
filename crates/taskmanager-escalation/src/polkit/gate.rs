//! Host-side, side-effect-free capability probing for the polkit crossings.

use crate::{EscalationAvailability, EscalationDenialReason, EscalationFeature, PrivilegeGate};

/// A real `PrivilegeGate` backed by the polkit/pkexec escalation path.
///
/// `probe` only checks whether an OS-native escalation path is present. It
/// never grants a capability and never launches an elevated process.
#[derive(Debug, Clone, Copy, Default)]
pub struct PolkitGate;

impl PolkitGate {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl PrivilegeGate for PolkitGate {
    fn probe(&self, feature: EscalationFeature) -> EscalationAvailability {
        match feature {
            EscalationFeature::IntelPmu => probe_intel_pmu(),
            EscalationFeature::PerProcessNet => probe_net_launcher(),
            EscalationFeature::ForeignProcessControl => probe_foreign_process_control(),
            EscalationFeature::MemorySmbios => probe_smbios_helper(),
            EscalationFeature::PackagePowerRapl => probe_rapl_helper(),
            EscalationFeature::CpuMsr => probe_msr_helper(),
            // Features without an operational helper stay on the honest
            // unprivileged default rather than claiming a helper we lack.
            other => EscalationAvailability::RequiresEscalation(other),
        }
    }
}

#[cfg(target_os = "linux")]
fn probe_intel_pmu() -> EscalationAvailability {
    probe_installed_crossing_at(
        EscalationFeature::IntelPmu,
        pkexec_location(),
        std::path::Path::new(PERF_HELPER_ACTION_INSTALLED),
        std::path::Path::new(super::PERF_HELPER_PATH),
        0,
    )
}

#[cfg(target_os = "linux")]
fn probe_foreign_process_control() -> EscalationAvailability {
    probe_installed_crossing_at(
        EscalationFeature::ForeignProcessControl,
        pkexec_location(),
        std::path::Path::new(PROCESS_CONTROL_ACTION_INSTALLED),
        std::path::Path::new(super::process_control::PROCESS_CONTROL_HELPER_PATH),
        0,
    )
}

#[cfg(not(target_os = "linux"))]
fn probe_foreign_process_control() -> EscalationAvailability {
    EscalationAvailability::Denied {
        reason: EscalationDenialReason::Unsupported,
    }
}

#[cfg(not(target_os = "linux"))]
fn probe_intel_pmu() -> EscalationAvailability {
    EscalationAvailability::Denied {
        reason: EscalationDenialReason::Unsupported,
    }
}

/// The installed polkit action for the net-launcher — the `.in` template
/// (`polkit/io.github.YellowWhiteBlackCat.TaskForest.net-launcher.policy.in`)
/// drops its suffix at install time (see `packaging/arch/PKGBUILD`). Probing
/// the ACTION FILE (not just the actions dir) matters: polkit resolves the
/// `pkexec` action by the annotated helper path, so an installed helper
/// without its action is unusable.
#[cfg(target_os = "linux")]
const NET_LAUNCHER_ACTION_INSTALLED: &str =
    "/usr/share/polkit-1/actions/io.github.YellowWhiteBlackCat.TaskForest.net-launcher.policy";

#[cfg(target_os = "linux")]
const PERF_HELPER_ACTION_INSTALLED: &str =
    "/usr/share/polkit-1/actions/io.github.YellowWhiteBlackCat.TaskForest.perf-helper.policy";

#[cfg(target_os = "linux")]
const PROCESS_CONTROL_ACTION_INSTALLED: &str =
    "/usr/share/polkit-1/actions/io.github.YellowWhiteBlackCat.TaskForest.process-control.policy";

#[cfg(target_os = "linux")]
const SMBIOS_ACTION_INSTALLED: &str =
    "/usr/share/polkit-1/actions/io.github.YellowWhiteBlackCat.TaskForest.smbios-helper.policy";

#[cfg(target_os = "linux")]
const RAPL_ACTION_INSTALLED: &str =
    "/usr/share/polkit-1/actions/io.github.YellowWhiteBlackCat.TaskForest.rapl-helper.policy";

#[cfg(target_os = "linux")]
const MSR_ACTION_INSTALLED: &str =
    "/usr/share/polkit-1/actions/io.github.YellowWhiteBlackCat.TaskForest.msr-helper.policy";

#[cfg(target_os = "linux")]
fn probe_net_launcher() -> EscalationAvailability {
    probe_installed_crossing_at(
        EscalationFeature::PerProcessNet,
        pkexec_location(),
        std::path::Path::new(NET_LAUNCHER_ACTION_INSTALLED),
        std::path::Path::new(super::net_launcher::NET_LAUNCHER_PATH),
        0,
    )
}

#[cfg(not(target_os = "linux"))]
fn probe_net_launcher() -> EscalationAvailability {
    EscalationAvailability::Denied {
        reason: EscalationDenialReason::Unsupported,
    }
}

#[cfg(target_os = "linux")]
fn probe_smbios_helper() -> EscalationAvailability {
    probe_installed_crossing_at(
        EscalationFeature::MemorySmbios,
        pkexec_location(),
        std::path::Path::new(SMBIOS_ACTION_INSTALLED),
        std::path::Path::new(super::smbios::SMBIOS_HELPER_PATH),
        0,
    )
}

#[cfg(not(target_os = "linux"))]
fn probe_smbios_helper() -> EscalationAvailability {
    EscalationAvailability::Denied {
        reason: EscalationDenialReason::Unsupported,
    }
}

#[cfg(target_os = "linux")]
fn probe_rapl_helper() -> EscalationAvailability {
    probe_installed_crossing_at(
        EscalationFeature::PackagePowerRapl,
        pkexec_location(),
        std::path::Path::new(RAPL_ACTION_INSTALLED),
        std::path::Path::new(super::rapl::RAPL_HELPER_PATH),
        0,
    )
}

#[cfg(not(target_os = "linux"))]
fn probe_rapl_helper() -> EscalationAvailability {
    EscalationAvailability::Denied {
        reason: EscalationDenialReason::Unsupported,
    }
}

#[cfg(target_os = "linux")]
fn probe_msr_helper() -> EscalationAvailability {
    probe_installed_crossing_at(
        EscalationFeature::CpuMsr,
        pkexec_location(),
        std::path::Path::new(MSR_ACTION_INSTALLED),
        std::path::Path::new(super::msr::MSR_HELPER_PATH),
        0,
    )
}

#[cfg(not(target_os = "linux"))]
fn probe_msr_helper() -> EscalationAvailability {
    EscalationAvailability::Denied {
        reason: EscalationDenialReason::Unsupported,
    }
}

/// The pure filesystem form of the net-launcher probe over EXPLICIT locations,
/// so tests can stage fixtures in temp dirs instead of asserting host state.
///
/// "Prompt available" requires all three pieces: `pkexec` resolvable on `PATH`,
/// the installed polkit action authorizing the launcher, and the launcher
/// binary at its annotated install path. Any missing piece is
/// [`EscalationDenialReason::HelperUnavailable`] — the honest "this host cannot
/// offer the prompt" answer, distinct from "prompt available, not yet used"
/// ([`EscalationAvailability::RequiresEscalation`]). Filesystem/PATH checks
/// only; no `pkexec` is executed and no prompt is ever raised by a probe.
#[cfg(all(test, target_os = "linux"))]
fn probe_net_launcher_at(
    pkexec: Option<std::path::PathBuf>,
    action: &std::path::Path,
    helper: &std::path::Path,
) -> EscalationAvailability {
    use std::os::unix::fs::MetadataExt;

    let expected_uid =
        std::fs::symlink_metadata(helper).map_or(u32::MAX, |metadata| metadata.uid());
    probe_installed_crossing_at(
        EscalationFeature::PerProcessNet,
        pkexec,
        action,
        helper,
        expected_uid,
    )
}

#[cfg(target_os = "linux")]
fn probe_installed_crossing_at(
    feature: EscalationFeature,
    pkexec: Option<std::path::PathBuf>,
    action: &std::path::Path,
    helper: &std::path::Path,
    expected_uid: u32,
) -> EscalationAvailability {
    if pkexec
        .as_deref()
        .is_some_and(|path| is_secure_regular_file(path, expected_uid, true))
        && is_secure_policy_file(action, helper, expected_uid)
        && is_secure_regular_file(helper, expected_uid, true)
    {
        EscalationAvailability::RequiresEscalation(feature)
    } else {
        EscalationAvailability::Denied {
            reason: EscalationDenialReason::HelperUnavailable,
        }
    }
}

#[cfg(target_os = "linux")]
fn pkexec_location() -> Option<std::path::PathBuf> {
    pkexec_in_path(&std::env::var_os("PATH").unwrap_or_default())
}

/// Resolve `pkexec` within an explicit `PATH`-style value: the first directory
/// whose `pkexec` entry is a FILE (a directory named `pkexec` is not an
/// executable candidate). Pure lookup — the binary is never executed. Split
/// out with the `PATH` value as a parameter so tests stay fixture-based.
#[cfg(target_os = "linux")]
fn pkexec_in_path(path: &std::ffi::OsStr) -> Option<std::path::PathBuf> {
    std::env::split_paths(path)
        .map(|dir| dir.join("pkexec"))
        .find(|candidate| is_executable_regular_file(candidate))
}

#[cfg(target_os = "linux")]
fn is_executable_regular_file(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.file_type().is_file() && metadata.permissions().mode() & 0o111 != 0
    })
}

#[cfg(target_os = "linux")]
fn is_secure_regular_file(path: &std::path::Path, expected_uid: u32, executable: bool) -> bool {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    std::fs::symlink_metadata(path).is_ok_and(|metadata| {
        let mode = metadata.permissions().mode();
        metadata.file_type().is_file()
            && metadata.uid() == expected_uid
            && mode & 0o022 == 0
            && (!executable || mode & 0o111 != 0)
    })
}

#[cfg(target_os = "linux")]
fn is_secure_policy_file(
    path: &std::path::Path,
    helper: &std::path::Path,
    expected_uid: u32,
) -> bool {
    const MAX_POLICY_BYTES: u64 = 64 * 1024;

    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if metadata.len() > MAX_POLICY_BYTES || !is_secure_regular_file(path, expected_uid, false) {
        return false;
    }
    let Some(helper) = helper.to_str() else {
        return false;
    };
    let annotation =
        format!("<annotate key=\"org.freedesktop.policykit.exec.path\">{helper}</annotate>");
    std::fs::read_to_string(path).is_ok_and(|policy| policy.contains(&annotation))
}

// Every probe fixture stages the Linux install tree (bin/pkexec, polkit-1
// actions, the launcher path), so the whole suite runs on Linux only.
#[cfg(all(test, target_os = "linux"))]
#[path = "../../tests/headless/escalation_polkit_gate.rs"]
mod tests;
