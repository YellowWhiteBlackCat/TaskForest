//! Typed, bounded service lifecycle control for the runtime-selected init.

use std::io;
use std::process::Command;
use std::time::Duration;

use taskmanager_core::{FailureKind, ServiceAction, ServiceId};
use taskmanager_platform_contract::ProviderFailure;

use super::log_fetch::permission_denied;
use super::target::{ResolvedServiceTarget, resolve_service_target};
use super::{InitSystem, SERVICE_COMMAND_TIMEOUT, ServiceManager};
use taskmanager_platform_portable::{BoundedCommandError, run_with_timeout};

#[derive(Clone, Debug, PartialEq, Eq)]
enum ControlCommandResult {
    Success(String),
    Failure(ProviderFailure),
}

trait ControlCommandRunner {
    fn run(&mut self, program: &str, args: &[&str]) -> ControlCommandResult;
}

struct NativeControlCommandRunner;

impl ControlCommandRunner for NativeControlCommandRunner {
    fn run(&mut self, program: &str, args: &[&str]) -> ControlCommandResult {
        run_control_command(program, args, SERVICE_COMMAND_TIMEOUT)
    }
}

impl ServiceManager {
    pub fn control_service(
        target: &ServiceId,
        action: ServiceAction,
    ) -> Result<(), ProviderFailure> {
        #[cfg(target_os = "linux")]
        {
            let resolved = resolve_service_target(target)?;
            let mut detector = || Self::detect_init();
            let mut runner = NativeControlCommandRunner;
            control_service_with(&resolved, action, &mut detector, &mut runner)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (target, action);
            Err(ProviderFailure::Unsupported)
        }
    }
}

fn control_service_with(
    target: &ResolvedServiceTarget,
    action: ServiceAction,
    detector: &mut impl FnMut() -> Result<InitSystem, FailureKind>,
    runner: &mut impl ControlCommandRunner,
) -> Result<(), ProviderFailure> {
    verify_detected_init(detector(), target.init())?;
    let native_name = target.native();
    match target.init() {
        InitSystem::Systemd => {
            if !native_name.ends_with(".service") {
                return Err(ProviderFailure::Rejected);
            }
            revalidate_systemd_target(native_name, runner)?;
        }
        InitSystem::Openrc => {
            revalidate_openrc_target(native_name, runner)?;
        }
        InitSystem::Unsupported => return Err(ProviderFailure::Unsupported),
    }
    verify_detected_init(detector(), target.init())?;
    match target.init() {
        InitSystem::Systemd => successful(runner.run(
            "systemctl",
            &[service_action_name(action), "--", native_name],
        )),
        InitSystem::Openrc => match action {
            ServiceAction::Start | ServiceAction::Stop | ServiceAction::Restart => {
                successful(runner.run("rc-service", &[native_name, service_action_name(action)]))
            }
            ServiceAction::Enable => {
                successful(runner.run("rc-update", &["add", native_name, "default"]))
            }
            ServiceAction::Disable => {
                successful(runner.run("rc-update", &["del", native_name, "default"]))
            }
        },
        InitSystem::Unsupported => Err(ProviderFailure::Unsupported),
    }
}

fn verify_detected_init(
    detection: Result<InitSystem, FailureKind>,
    expected: InitSystem,
) -> Result<(), ProviderFailure> {
    match detection {
        Ok(actual) if actual == expected => Ok(()),
        Ok(InitSystem::Unsupported) => Err(ProviderFailure::Unsupported),
        Ok(_) => Err(ProviderFailure::IdentityChanged),
        Err(failure) => Err(ProviderFailure::from_kind(failure)),
    }
}

fn revalidate_systemd_target(
    unit: &str,
    runner: &mut impl ControlCommandRunner,
) -> Result<(), ProviderFailure> {
    match runner.run(
        "systemctl",
        &["show", "--property=LoadState", "--value", "--", unit],
    ) {
        ControlCommandResult::Success(stdout) if stdout.trim() == "loaded" => Ok(()),
        ControlCommandResult::Success(_) => Err(ProviderFailure::IdentityChanged),
        ControlCommandResult::Failure(ProviderFailure::Rejected) => {
            Err(ProviderFailure::IdentityChanged)
        }
        ControlCommandResult::Failure(failure) => Err(failure),
    }
}

fn revalidate_openrc_target(
    native_name: &str,
    runner: &mut impl ControlCommandRunner,
) -> Result<(), ProviderFailure> {
    match runner.run("rc-service", &["--exists", native_name]) {
        ControlCommandResult::Success(_) => Ok(()),
        ControlCommandResult::Failure(ProviderFailure::Rejected) => {
            Err(ProviderFailure::IdentityChanged)
        }
        ControlCommandResult::Failure(failure) => Err(failure),
    }
}

fn successful(result: ControlCommandResult) -> Result<(), ProviderFailure> {
    match result {
        ControlCommandResult::Success(_) => Ok(()),
        ControlCommandResult::Failure(failure) => Err(failure),
    }
}

const fn service_action_name(action: ServiceAction) -> &'static str {
    match action {
        ServiceAction::Start => "start",
        ServiceAction::Stop => "stop",
        ServiceAction::Restart => "restart",
        ServiceAction::Enable => "enable",
        ServiceAction::Disable => "disable",
    }
}

fn run_control_command(program: &str, args: &[&str], timeout: Duration) -> ControlCommandResult {
    let mut command = Command::new(program);
    command.args(args);
    match run_with_timeout(&mut command, timeout) {
        Ok(output) if output.status.success() => {
            ControlCommandResult::Success(String::from_utf8_lossy(&output.stdout).into_owned())
        }
        Ok(output) => ControlCommandResult::Failure(classify_nonzero_exit(&output.stderr)),
        Err(BoundedCommandError::Spawn(error)) => {
            ControlCommandResult::Failure(classify_spawn_error(error.kind()))
        }
        Err(BoundedCommandError::TimedOut | BoundedCommandError::ReaderTimedOut) => {
            ControlCommandResult::Failure(ProviderFailure::TimedOut)
        }
        Err(
            BoundedCommandError::ReaderStart(_)
            | BoundedCommandError::ReaderFailed
            | BoundedCommandError::ProcessTree
            | BoundedCommandError::OutputTooLarge,
        ) => ControlCommandResult::Failure(ProviderFailure::ProviderFault),
    }
}

fn classify_nonzero_exit(stderr: &[u8]) -> ProviderFailure {
    let stderr = String::from_utf8_lossy(stderr);
    let lower = stderr.to_ascii_lowercase();
    if permission_denied(&stderr) {
        ProviderFailure::PermissionDenied
    } else if [
        "does not exist",
        "not found",
        "no such unit",
        "could not be found",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
    {
        ProviderFailure::IdentityChanged
    } else {
        ProviderFailure::Rejected
    }
}

const fn classify_spawn_error(kind: io::ErrorKind) -> ProviderFailure {
    match kind {
        io::ErrorKind::NotFound => ProviderFailure::MissingDependency,
        io::ErrorKind::PermissionDenied => ProviderFailure::PermissionDenied,
        io::ErrorKind::TimedOut => ProviderFailure::TimedOut,
        _ => ProviderFailure::ProviderFault,
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_services_control_tests.rs"]
mod tests;
