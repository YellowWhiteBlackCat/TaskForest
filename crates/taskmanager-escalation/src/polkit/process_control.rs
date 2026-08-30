//! Fixed-argument pkexec crossing for foreign-process control.
//!
//! The provider first attempts the normal unprivileged syscall. On a typed
//! permission denial it may invoke this one-operation helper after the user
//! has already confirmed the destructive action in the UI. The helper receives
//! only PID + exact `/proc` start token + a closed operation vocabulary; it
//! never receives a shell command or an arbitrary executable path.

#![forbid(unsafe_code)]

use std::io;

use super::json_reader::{Json, JsonReader};
use super::{EscalationDenialReason, HelperOutput};

#[cfg(target_os = "linux")]
pub(super) const PROCESS_CONTROL_HELPER_PATH: &str =
    "/usr/libexec/taskforest-process-control-helper";

/// The provider-native identity supplied to the privileged helper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForeignProcessControlTarget {
    pid: u32,
    start_token: u64,
}

impl ForeignProcessControlTarget {
    /// Build a target only when both identity components are non-zero.
    #[must_use]
    pub const fn new(pid: u32, start_token: u64) -> Option<Self> {
        if pid == 0 || start_token == 0 {
            None
        } else {
            Some(Self { pid, start_token })
        }
    }

    #[must_use]
    pub const fn pid(self) -> u32 {
        self.pid
    }

    #[must_use]
    pub const fn start_token(self) -> u64 {
        self.start_token
    }
}

/// Closed operation vocabulary accepted by the process-control helper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForeignProcessControlOperation {
    End,
    Kill,
    Suspend,
    Resume,
    SetPriority(i32),
    Signal(ForeignProcessSignal),
    SetAffinity(Vec<u32>),
}

impl ForeignProcessControlOperation {
    /// The helper's fixed wire form for this operation (`end`, `kill`,
    /// `priority:<nice>`, ...). `pub` because the Windows runas transport
    /// (ADR-035 stage 2, driven from `taskmanager-platform-windows`) launches
    /// the SAME helper binary with the SAME argument vocabulary; one authority
    /// defines the wire form for both crossings.
    #[must_use]
    pub fn argument(&self) -> String {
        match self {
            Self::End => "end".to_owned(),
            Self::Kill => "kill".to_owned(),
            Self::Suspend => "suspend".to_owned(),
            Self::Resume => "resume".to_owned(),
            Self::SetPriority(nice) => format!("priority:{nice}"),
            Self::Signal(signal) => format!("signal:{}", signal.argument()),
            Self::SetAffinity(cpus) => {
                let values = cpus.iter().map(u32::to_string).collect::<Vec<_>>();
                format!("affinity:{}", values.join(","))
            }
        }
    }
}

/// Signals exposed by the existing process context-menu path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForeignProcessSignal {
    Terminate,
    Kill,
    Stop,
    Continue,
    Hangup,
    Interrupt,
    User1,
    User2,
}

impl ForeignProcessSignal {
    const fn argument(self) -> &'static str {
        match self {
            Self::Terminate => "terminate",
            Self::Kill => "kill",
            Self::Stop => "stop",
            Self::Continue => "continue",
            Self::Hangup => "hangup",
            Self::Interrupt => "interrupt",
            Self::User1 => "user1",
            Self::User2 => "user2",
        }
    }
}

/// Typed failure emitted by the helper after it has started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForeignProcessControlFailure {
    IdentityChanged,
    PermissionDenied,
    Unsupported,
    Rejected,
    OperationFailed,
}

/// Result of one feature-specific process-control escalation attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForeignProcessControlOutcome {
    /// The helper validated the identity and applied exactly one operation.
    Applied,
    /// The helper ran but could not apply the requested operation.
    Failed {
        kind: ForeignProcessControlFailure,
        detail: String,
    },
    /// The OS-native prompt/helper crossing did not produce a helper contract.
    Unavailable {
        reason: EscalationDenialReason,
        detail: String,
    },
}

/// Injectable process seam for the fixed process-control helper.
pub trait ForeignProcessControlProcess {
    /// Run one helper operation and capture its stdout plus exit status.
    fn run(
        &self,
        target: ForeignProcessControlTarget,
        operation: &ForeignProcessControlOperation,
    ) -> io::Result<HelperOutput>;
}

/// Production Linux driver for the feature-specific process-control helper.
#[derive(Debug, Clone, Copy, Default)]
pub struct PkexecForeignProcessControl;

impl PkexecForeignProcessControl {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[cfg(target_os = "linux")]
impl ForeignProcessControlProcess for PkexecForeignProcessControl {
    fn run(
        &self,
        target: ForeignProcessControlTarget,
        operation: &ForeignProcessControlOperation,
    ) -> io::Result<HelperOutput> {
        use std::process::{Command, Stdio};

        let mut command = Command::new("pkexec");
        command
            .arg(PROCESS_CONTROL_HELPER_PATH)
            .arg(target.pid().to_string())
            .arg(target.start_token().to_string())
            .arg(operation.argument())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        // Bounded run: the "end process" path must never park forever on an
        // abandoned polkit dialog, and a runaway stdout is capped (it then
        // flows through the NotContract classification below).
        let output = super::bounded_runner::run_bounded(
            &mut command,
            super::bounded_runner::INTERACTIVE_PKEXEC_DEADLINE,
        )
        .map_err(|error| error.into_io_error("the pkexec process-control helper"))?;
        Ok(HelperOutput {
            status_code: output.status_code,
            stdout: output.stdout,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ParsedOutput {
    Applied,
    Failed {
        kind: ForeignProcessControlFailure,
        detail: String,
    },
    NotContract,
}

/// Parse one helper contract message. `pub(crate)`: the single authority for
/// the process-control helper contract — the Windows UAC reply channel
/// (`crate::uac`) classifies its payloads by this same rule (ADR-035
/// "非契约内容按既有 NotContract 语义分类"), never a second parser.
pub(crate) fn parse_output(stdout: &str) -> ParsedOutput {
    let Ok(json) = JsonReader::parse(stdout) else {
        return ParsedOutput::NotContract;
    };
    if json.get("status").and_then(Json::as_str) == Some("applied") {
        if schema_is_one(&json) && json.get("operation").and_then(Json::as_str).is_some() {
            ParsedOutput::Applied
        } else {
            ParsedOutput::NotContract
        }
    } else if json.get("status").and_then(Json::as_str) == Some("error") {
        let Some(kind) = json
            .get("kind")
            .and_then(Json::as_str)
            .and_then(parse_failure)
        else {
            return ParsedOutput::NotContract;
        };
        let Some(detail) = json.get("detail").and_then(Json::as_str) else {
            return ParsedOutput::NotContract;
        };
        if schema_is_one(&json) {
            ParsedOutput::Failed {
                kind,
                detail: detail.to_owned(),
            }
        } else {
            ParsedOutput::NotContract
        }
    } else {
        ParsedOutput::NotContract
    }
}

fn schema_is_one(json: &Json) -> bool {
    matches!(
        json.get("schema"),
        Some(Json::Number(value)) if value.is_finite() && *value == 1.0
    )
}

fn parse_failure(raw: &str) -> Option<ForeignProcessControlFailure> {
    Some(match raw {
        "identity_changed" => ForeignProcessControlFailure::IdentityChanged,
        "permission_denied" => ForeignProcessControlFailure::PermissionDenied,
        "unsupported" => ForeignProcessControlFailure::Unsupported,
        "rejected" => ForeignProcessControlFailure::Rejected,
        "operation_failed" => ForeignProcessControlFailure::OperationFailed,
        _ => return None,
    })
}

/// Invoke one process-control helper through an injected process seam.
pub fn invoke_foreign_process_control_with<P: ForeignProcessControlProcess>(
    process: &P,
    target: ForeignProcessControlTarget,
    operation: ForeignProcessControlOperation,
) -> ForeignProcessControlOutcome {
    let output = match process.run(target, &operation) {
        Ok(output) => output,
        Err(error) => {
            let detail = if error.kind() == io::ErrorKind::TimedOut {
                format!(
                    "the pkexec process-control helper crossing was killed at its deadline: {error}"
                )
            } else {
                format!("could not spawn the pkexec process-control helper: {error}")
            };
            return ForeignProcessControlOutcome::Unavailable {
                reason: EscalationDenialReason::HelperUnavailable,
                detail,
            };
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    match parse_output(&stdout) {
        ParsedOutput::Applied => ForeignProcessControlOutcome::Applied,
        ParsedOutput::Failed { kind, detail } => {
            ForeignProcessControlOutcome::Failed { kind, detail }
        }
        ParsedOutput::NotContract => {
            let (reason, detail) = super::classify_pkexec_no_contract(
                output.status_code,
                process_control_helper_path_for_detail(),
                &stdout,
            );
            ForeignProcessControlOutcome::Unavailable { reason, detail }
        }
    }
}

#[cfg(target_os = "linux")]
fn process_control_helper_path_for_detail() -> &'static str {
    PROCESS_CONTROL_HELPER_PATH
}

#[cfg(target_os = "windows")]
fn process_control_helper_path_for_detail() -> &'static str {
    "taskmanager-process-control-helper.exe"
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn process_control_helper_path_for_detail() -> &'static str {
    "the annotated helper path"
}

/// Invoke the production Linux helper.
#[cfg(target_os = "linux")]
pub fn invoke_foreign_process_control(
    target: ForeignProcessControlTarget,
    operation: ForeignProcessControlOperation,
) -> ForeignProcessControlOutcome {
    invoke_foreign_process_control_with(&PkexecForeignProcessControl::new(), target, operation)
}

/// The pkexec/polkit crossing is Linux-only. On Windows the foreign-process
/// crossing is the UAC transport (`crate::uac`, ADR-035) driven from
/// `taskmanager-platform-windows`; a normal child process would inherit the
/// application's token and is not an escalation mechanism, so this polkit
/// entry fails closed without spawning the helper binary.
#[cfg(target_os = "windows")]
pub fn invoke_foreign_process_control(
    _target: ForeignProcessControlTarget,
    _operation: ForeignProcessControlOperation,
) -> ForeignProcessControlOutcome {
    windows_foreign_process_control_unavailable()
}

#[cfg(any(target_os = "windows", test))]
fn windows_foreign_process_control_unavailable() -> ForeignProcessControlOutcome {
    ForeignProcessControlOutcome::Unavailable {
        reason: EscalationDenialReason::Unsupported,
        detail: "the pkexec crossing is Linux-only; the Windows crossing is the UAC transport \
                 (crate::uac) driven by taskmanager-platform-windows"
            .to_owned(),
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn invoke_foreign_process_control(
    _target: ForeignProcessControlTarget,
    _operation: ForeignProcessControlOperation,
) -> ForeignProcessControlOutcome {
    ForeignProcessControlOutcome::Unavailable {
        reason: EscalationDenialReason::Unsupported,
        detail: "foreign process-control escalation is unsupported on this platform".to_owned(),
    }
}

#[cfg(test)]
#[path = "../../tests/headless/escalation_polkit_process_control.rs"]
mod tests;
