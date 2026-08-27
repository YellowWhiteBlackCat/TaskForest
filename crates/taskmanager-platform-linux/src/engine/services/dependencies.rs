//! Typed, bounded dependency lookup for the runtime-selected service backend.

use std::io;
use std::process::Command;
use std::time::Duration;

use taskmanager_platform_contract::ProviderFailure;

use super::log_fetch::permission_denied;
use super::target::resolve_active_service_target;
use super::{
    InitSystem, SERVICE_COMMAND_TIMEOUT, ServiceDeps, ServiceManager, parse_systemctl_show_deps,
};
use taskmanager_platform_portable::{BoundedCommandError, run_with_timeout};

const SYSTEMD_DEPENDENCY_PROPERTIES: &str =
    "Requires,Wants,Requisite,BindsTo,PartOf,Conflicts,Before,After,WantedBy,RequiredBy,UpheldBy";

#[derive(Clone, Debug, PartialEq, Eq)]
enum DependencyCommandResult {
    Success(String),
    Failure(ProviderFailure),
}

trait DependencyCommandRunner {
    fn run(&mut self, program: &str, args: &[&str]) -> DependencyCommandResult;
}

struct NativeDependencyCommandRunner;

impl DependencyCommandRunner for NativeDependencyCommandRunner {
    fn run(&mut self, program: &str, args: &[&str]) -> DependencyCommandResult {
        run_dependency_command(program, args, SERVICE_COMMAND_TIMEOUT)
    }
}

impl ServiceManager {
    /// Fetch dependency metadata only when the details surface requests it.
    ///
    /// An empty [`ServiceDeps`] is authoritative only when `systemctl show`
    /// completed successfully. Command and backend failures stay typed so the
    /// application can emit `DependenciesUnavailable` and retry later.
    pub fn fetch_deps(
        target: &taskmanager_core::ServiceId,
    ) -> Result<ServiceDeps, ProviderFailure> {
        #[cfg(target_os = "linux")]
        {
            let target = resolve_active_service_target(target)?;
            let mut runner = NativeDependencyCommandRunner;
            Self::fetch_deps_with(target.init(), target.native(), &mut runner)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = target;
            Err(ProviderFailure::Unsupported)
        }
    }

    fn fetch_deps_with(
        init: InitSystem,
        name: &str,
        runner: &mut impl DependencyCommandRunner,
    ) -> Result<ServiceDeps, ProviderFailure> {
        if init != InitSystem::Systemd {
            return Err(ProviderFailure::Unsupported);
        }

        if !name.ends_with(".service") {
            return Err(ProviderFailure::Rejected);
        }
        match runner.run(
            "systemctl",
            &["show", "-p", SYSTEMD_DEPENDENCY_PROPERTIES, "--", name],
        ) {
            DependencyCommandResult::Success(output) => {
                Ok(parse_systemctl_show_deps(output.as_str()))
            }
            DependencyCommandResult::Failure(error) => Err(error),
        }
    }
}

fn run_dependency_command(
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> DependencyCommandResult {
    let mut command = Command::new(program);
    command.args(args);
    match run_with_timeout(&mut command, timeout) {
        Ok(output) if output.status.success() => match String::from_utf8(output.stdout) {
            Ok(stdout) => DependencyCommandResult::Success(stdout),
            Err(_) => DependencyCommandResult::Failure(ProviderFailure::ProviderFault),
        },
        Ok(output) => DependencyCommandResult::Failure(classify_nonzero_exit(&output.stderr)),
        Err(BoundedCommandError::Spawn(error)) => {
            DependencyCommandResult::Failure(classify_spawn_error(error.kind()))
        }
        Err(BoundedCommandError::TimedOut | BoundedCommandError::ReaderTimedOut) => {
            DependencyCommandResult::Failure(ProviderFailure::TimedOut)
        }
        Err(
            BoundedCommandError::ReaderStart(_)
            | BoundedCommandError::ReaderFailed
            | BoundedCommandError::ProcessTree
            | BoundedCommandError::OutputTooLarge,
        ) => DependencyCommandResult::Failure(ProviderFailure::ProviderFault),
    }
}

fn classify_nonzero_exit(stderr: &[u8]) -> ProviderFailure {
    if permission_denied(&String::from_utf8_lossy(stderr)) {
        ProviderFailure::PermissionDenied
    } else {
        ProviderFailure::Rejected
    }
}

const fn classify_spawn_error(kind: io::ErrorKind) -> ProviderFailure {
    match kind {
        io::ErrorKind::NotFound => ProviderFailure::MissingDependency,
        io::ErrorKind::PermissionDenied => ProviderFailure::PermissionDenied,
        _ => ProviderFailure::ProviderFault,
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_services_dependencies_tests.rs"]
mod tests;
