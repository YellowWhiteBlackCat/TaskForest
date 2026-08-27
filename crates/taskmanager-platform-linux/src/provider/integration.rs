//! Linux desktop and shell integration providers.

use taskmanager_core::{DesktopAppearance, FrozenProcessIdentity};
use taskmanager_core::{SetupScriptAction, SetupScriptEvent, SetupScriptInfo};
use taskmanager_escalation::polkit::{
    SetupScriptFailure, SetupScriptOperation, SetupScriptOutcome, invoke_setup_script,
};
use taskmanager_platform_contract::{CompositeSourceSnapshot, ProviderFailure};
use taskmanager_platform_provider::{
    CommandLaunchProvider, DesktopAppearanceProvider, DesktopNotificationProvider,
    ResourceRevealProvider, SetupScriptProvider, UrlOpenProvider,
};

use super::process_target::validate_process_identity;
use crate::engine::process::{ProcessManager, validate_exact_start_token};

const SETUP_SCRIPT_PATH: &str = "/usr/share/taskmanager/setup/99-taskmanager.rules";
const SETUP_HELPER_PATH: &str = "/usr/libexec/taskmanager-setup-helper";

pub(super) struct NativeCommandLaunchProvider;

impl CommandLaunchProvider for NativeCommandLaunchProvider {
    fn run_command(&mut self, command: &str) -> Result<u32, ProviderFailure> {
        ProcessManager::run(command).map_err(|error| classify_shell_error(&error))
    }
}

pub(super) struct NativeResourceRevealProvider {
    pub(super) process_manager: ProcessManager,
}

impl ResourceRevealProvider for NativeResourceRevealProvider {
    fn reveal_process(
        &mut self,
        target: &FrozenProcessIdentity,
        cached_executable: Option<&std::path::Path>,
    ) -> Result<(), ProviderFailure> {
        validate_process_identity(&mut self.process_manager, target)?;
        let executable = cached_executable
            .map(std::path::Path::to_path_buf)
            .or_else(|| crate::engine::process::read_exe_path(target.pid))
            .ok_or(ProviderFailure::TemporarilyUnavailable)?;
        validate_process_identity(&mut self.process_manager, target)?;
        validate_exact_start_token(target).map_err(ProviderFailure::from_kind)?;
        crate::engine::process::open_file_location(&executable)
            .map_err(|error| classify_shell_error(&error))
    }
}

pub(super) struct NativeUrlOpenProvider;

impl UrlOpenProvider for NativeUrlOpenProvider {
    fn open_url(&mut self, url: &str) -> Result<(), ProviderFailure> {
        ProcessManager::xdg_open(url).map_err(|error| classify_shell_error(&error))
    }
}

pub(super) struct NativeDesktopAppearanceProvider;

impl DesktopAppearanceProvider for NativeDesktopAppearanceProvider {
    fn observe(&mut self) -> Result<CompositeSourceSnapshot<DesktopAppearance>, ProviderFailure> {
        Ok(crate::engine::desktop_appearance::observe_desktop_appearance())
    }
}

/// Desktop notification delivery over the freedesktop DBus service (BN-07).
/// Pure safe Rust through `zbus`; no session bus or no notification service
/// is typed `MissingDependency`, a refused call `TemporarilyUnavailable` —
/// never a fabricated success.
pub(super) struct NativeDesktopNotificationProvider;

impl DesktopNotificationProvider for NativeDesktopNotificationProvider {
    fn notify(
        &mut self,
        title: &str,
        body: &str,
        _severity: taskmanager_core::AlertSeverity,
        target: &str,
    ) -> Result<(), ProviderFailure> {
        use std::collections::HashMap;

        let connection = zbus::blocking::Connection::session()
            .map_err(|_| ProviderFailure::MissingDependency)?;
        let body = if target.is_empty() {
            body.to_owned()
        } else {
            format!("{target} — {body}")
        };
        let hints = HashMap::<&str, zbus::zvariant::Value<'_>>::new();
        let result = connection.call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &(
                "TaskForest",
                0u32,
                "taskmanager",
                title,
                &body,
                Vec::<&str>::new(),
                hints,
                -1i32,
            ),
        );
        match result {
            Ok(_) => Ok(()),
            Err(_) => Err(ProviderFailure::TemporarilyUnavailable),
        }
    }
}

/// Linux's fixed First Run setup asset. The provider exposes metadata only for
/// the packaged asset and routes actions through the dedicated escalation
/// helper; arbitrary shell launch is deliberately not reused.
pub(super) struct NativeSetupScriptProvider;

impl NativeSetupScriptProvider {
    fn info() -> SetupScriptInfo {
        SetupScriptInfo {
            path: std::path::PathBuf::from(SETUP_SCRIPT_PATH),
            run_command: format!("pkexec {SETUP_HELPER_PATH} install"),
            revert_command: format!("pkexec {SETUP_HELPER_PATH} revert"),
        }
    }

    fn require_info() -> Result<SetupScriptInfo, ProviderFailure> {
        let path = std::path::Path::new(SETUP_SCRIPT_PATH);
        if !path.is_file() {
            return Err(ProviderFailure::MissingDependency);
        }
        if !std::path::Path::new(SETUP_HELPER_PATH).is_file() {
            return Err(ProviderFailure::MissingDependency);
        }
        Ok(Self::info())
    }
}

impl SetupScriptProvider for NativeSetupScriptProvider {
    fn perform(&mut self, action: SetupScriptAction) -> Result<SetupScriptEvent, ProviderFailure> {
        match action {
            SetupScriptAction::Observe => {
                if !std::path::Path::new(SETUP_SCRIPT_PATH).is_file() {
                    return Ok(SetupScriptEvent::Observed(None));
                }
                let info = Self::require_info()?;
                Ok(SetupScriptEvent::Observed(Some(info)))
            }
            SetupScriptAction::View => {
                let info = Self::require_info()?;
                ProcessManager::xdg_open(
                    info.path
                        .to_str()
                        .ok_or(ProviderFailure::TemporarilyUnavailable)?,
                )
                .map_err(|error| classify_shell_error(&error))?;
                Ok(SetupScriptEvent::ActionCompleted { action })
            }
            SetupScriptAction::Run | SetupScriptAction::Revert => {
                let _ = Self::require_info()?;
                let operation = if action == SetupScriptAction::Run {
                    SetupScriptOperation::Install
                } else {
                    SetupScriptOperation::Revert
                };
                match invoke_setup_script(operation) {
                    SetupScriptOutcome::Success => Ok(SetupScriptEvent::ActionCompleted { action }),
                    SetupScriptOutcome::Failed { kind, .. } => Err(classify_setup_failure(kind)),
                }
            }
            SetupScriptAction::Restart => {
                restart_application().map_err(|error| classify_shell_error(&error))?;
                Ok(SetupScriptEvent::ActionCompleted { action })
            }
        }
    }
}

fn restart_application() -> Result<(), String> {
    use std::process::{Command, Stdio};

    let executable = std::env::current_exe()
        .map_err(|error| format!("cannot resolve current TaskForest executable: {error}"))?;
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    Command::new(executable)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("cannot relaunch TaskForest: {error}"))
}

fn classify_setup_failure(failure: SetupScriptFailure) -> ProviderFailure {
    match failure {
        SetupScriptFailure::PermissionDenied => ProviderFailure::PermissionDenied,
        SetupScriptFailure::HelperUnavailable | SetupScriptFailure::MissingDependency => {
            ProviderFailure::MissingDependency
        }
        SetupScriptFailure::Rejected => ProviderFailure::Rejected,
        SetupScriptFailure::ProviderFault => ProviderFailure::ProviderFault,
    }
}

fn classify_shell_error(error: &str) -> ProviderFailure {
    let lower = error.to_ascii_lowercase();
    if lower.contains("permission denied") {
        ProviderFailure::PermissionDenied
    } else if lower.contains("not supported")
        || lower.contains("no such file")
        || lower.contains("not found")
    {
        ProviderFailure::TemporarilyUnavailable
    } else {
        ProviderFailure::Rejected
    }
}

#[cfg(test)]
#[path = "../../tests/headless/linux_provider_integration_tests.rs"]
mod tests;
