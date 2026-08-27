//! Runtime-only selection of the active Linux init implementation.

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use taskmanager_core::FailureKind;
use taskmanager_platform_contract::ProviderFailure;

use super::ServiceManager;

/// Provider-private identity of the init implementation active for this request.
///
/// This is deliberately not a shared-core variant: other OS adapters have
/// different supervisors, while Linux can switch implementation without
/// changing the product artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InitSystem {
    Systemd,
    Openrc,
    Unsupported,
}

impl ServiceManager {
    /// Re-probe the active init runtime for every operation.
    ///
    /// Init selection is intentionally not cached. Containers can be reattached,
    /// `/proc` can be mounted later, and test/target environments can replace
    /// their service runtime while the application remains alive.
    pub fn detect_init() -> Result<InitSystem, FailureKind> {
        probe_init()
    }
}

#[cfg(target_os = "linux")]
fn probe_init() -> Result<InitSystem, FailureKind> {
    let openrc_runtime_active =
        Path::new("/run/openrc/softlevel").is_file() || Path::new("/run/openrc").is_dir();
    let openrc_binary_installed = Path::new("/sbin/openrc").is_file();
    match fs::read_to_string("/proc/1/comm") {
        Ok(pid_one) => Ok(classify_init(
            Some(pid_one.as_str()),
            openrc_runtime_active,
            openrc_binary_installed,
        )),
        Err(_) if openrc_runtime_active => Ok(InitSystem::Openrc),
        Err(error) => Err(classify_probe_error(error.kind())),
    }
}

#[cfg(not(target_os = "linux"))]
fn probe_init() -> Result<InitSystem, FailureKind> {
    Ok(InitSystem::Unsupported)
}

pub(super) fn classify_init(
    pid_one_comm: Option<&str>,
    openrc_runtime_active: bool,
    _openrc_binary_installed: bool,
) -> InitSystem {
    if pid_one_comm.is_some_and(|comm| comm.trim() == "systemd") {
        InitSystem::Systemd
    } else if pid_one_comm.is_some_and(|comm| matches!(comm.trim(), "openrc" | "openrc-init"))
        || openrc_runtime_active
    {
        InitSystem::Openrc
    } else {
        InitSystem::Unsupported
    }
}

const fn classify_probe_error(kind: io::ErrorKind) -> FailureKind {
    match kind {
        io::ErrorKind::NotFound => FailureKind::MissingDependency,
        io::ErrorKind::PermissionDenied => FailureKind::PermissionDenied,
        io::ErrorKind::TimedOut => FailureKind::TimedOut,
        _ => FailureKind::ProviderFault,
    }
}

pub(super) const fn detection_provider_failure(failure: FailureKind) -> ProviderFailure {
    match failure {
        FailureKind::Unsupported => ProviderFailure::Unsupported,
        FailureKind::RequiresEscalation => ProviderFailure::RequiresEscalation,
        FailureKind::PermissionDenied => ProviderFailure::PermissionDenied,
        FailureKind::MissingDependency => ProviderFailure::MissingDependency,
        FailureKind::TimedOut => ProviderFailure::TimedOut,
        FailureKind::IdentityChanged => ProviderFailure::IdentityChanged,
        FailureKind::TemporarilyUnavailable => ProviderFailure::TemporarilyUnavailable,
        FailureKind::Rejected => ProviderFailure::Rejected,
        FailureKind::ProviderFault => ProviderFailure::ProviderFault,
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_services_init_runtime_tests.rs"]
mod tests;
