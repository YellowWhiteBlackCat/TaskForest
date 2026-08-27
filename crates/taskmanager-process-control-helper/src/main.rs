//! `taskmanager-process-control-helper` — one foreign-process operation per
//! polkit/pkexec authorization.
//!
//! The main application remains unprivileged. When a user-confirmed process
//! action receives a kernel permission denial, the Linux provider may invoke
//! this helper through the feature-specific polkit action. The helper accepts
//! only a PID, the provider-native `/proc/<pid>/stat` start token, and one
//! fixed operation. Signal operations open a pidfd FIRST — pinning the exact
//! process instance in the kernel — then re-read the token, then signal
//! through the pinned handle, so a pid that exits and is recycled between
//! check and effect can never inherit the user's intent. Priority and
//! affinity have no pidfd syscall variant: their token re-read sits
//! immediately before the syscall (audited residual window, bounded to a
//! mis-set niceness/mask on a recycled pid, never death).
//!
//! Args:
//! `taskmanager-process-control-helper <pid> <start-token> <operation>`
//!
//! Operations are `end`, `kill`, `suspend`, `resume`, `priority:<nice>`,
//! `signal:<name>`, or `affinity:<cpu,cpu,...>`. The helper performs no shell
//! expansion, reads no user-controlled path, and emits one typed JSON result.

#![forbid(unsafe_code)]
// The control surface exists behind unix and windows gates; on other targets
// this binary compiles down to the typed-unavailable entry.
#![cfg_attr(not(any(unix, windows)), allow(dead_code))]

use serde::Serialize;
// Consumed only by the unix-gated control paths below.
#[cfg(unix)]
use std::io;
use std::process::ExitCode;

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Operation {
    End,
    Kill,
    Suspend,
    Resume,
    Priority(i32),
    Signal(SignalName),
    Affinity(Vec<u32>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SignalName {
    Terminate,
    Kill,
    Stop,
    Continue,
    Hangup,
    Interrupt,
    User1,
    User2,
}

impl SignalName {
    fn parse(raw: &str) -> Result<Self, HelperError> {
        match raw {
            "terminate" => Ok(Self::Terminate),
            "kill" => Ok(Self::Kill),
            "stop" => Ok(Self::Stop),
            "continue" => Ok(Self::Continue),
            "hangup" => Ok(Self::Hangup),
            "interrupt" => Ok(Self::Interrupt),
            "user1" => Ok(Self::User1),
            "user2" => Ok(Self::User2),
            _ => Err(HelperError::Rejected("unknown signal".to_owned())),
        }
    }

    const fn contract_name(self) -> &'static str {
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

impl Operation {
    fn parse(raw: &str) -> Result<Self, HelperError> {
        match raw {
            "end" => Ok(Self::End),
            "kill" => Ok(Self::Kill),
            "suspend" => Ok(Self::Suspend),
            "resume" => Ok(Self::Resume),
            value if value.starts_with("priority:") => {
                let raw_nice = value.strip_prefix("priority:").unwrap_or_default();
                let nice = raw_nice
                    .parse::<i32>()
                    .map_err(|_| HelperError::Rejected("invalid priority".to_owned()))?;
                if !(-20..=19).contains(&nice) {
                    return Err(HelperError::Rejected(
                        "priority must be between -20 and 19".to_owned(),
                    ));
                }
                Ok(Self::Priority(nice))
            }
            value if value.starts_with("signal:") => {
                let signal = value.strip_prefix("signal:").unwrap_or_default();
                Ok(Self::Signal(SignalName::parse(signal)?))
            }
            value if value.starts_with("affinity:") => {
                let raw_cpus = value.strip_prefix("affinity:").unwrap_or_default();
                let mut cpus = Vec::new();
                for raw_cpu in raw_cpus.split(',') {
                    let cpu = raw_cpu
                        .parse::<u32>()
                        .map_err(|_| HelperError::Rejected("invalid CPU id".to_owned()))?;
                    cpus.push(cpu);
                }
                if cpus.is_empty() {
                    return Err(HelperError::Rejected(
                        "affinity must contain at least one CPU".to_owned(),
                    ));
                }
                cpus.sort_unstable();
                cpus.dedup();
                Ok(Self::Affinity(cpus))
            }
            _ => Err(HelperError::Rejected(
                "unknown process operation".to_owned(),
            )),
        }
    }

    fn contract_name(&self) -> String {
        match self {
            Self::End => "end".to_owned(),
            Self::Kill => "kill".to_owned(),
            Self::Suspend => "suspend".to_owned(),
            Self::Resume => "resume".to_owned(),
            Self::Priority(nice) => format!("priority:{nice}"),
            Self::Signal(signal) => format!("signal:{}", signal.contract_name()),
            Self::Affinity(cpus) => {
                let values = cpus.iter().map(u32::to_string).collect::<Vec<_>>();
                format!("affinity:{}", values.join(","))
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum HelperError {
    ArgError(String),
    IdentityChanged(String),
    PermissionDenied(String),
    Unsupported(String),
    Rejected(String),
    OperationFailed(String),
}

impl HelperError {
    const fn kind(&self) -> &'static str {
        match self {
            Self::ArgError(_) => "arg_error",
            Self::IdentityChanged(_) => "identity_changed",
            Self::PermissionDenied(_) => "permission_denied",
            Self::Unsupported(_) => "unsupported",
            Self::Rejected(_) => "rejected",
            Self::OperationFailed(_) => "operation_failed",
        }
    }

    const fn exit_code(&self) -> u8 {
        match self {
            Self::ArgError(_) => 64,
            Self::IdentityChanged(_) => 3,
            Self::PermissionDenied(_) => 2,
            Self::Unsupported(_) => 69,
            Self::Rejected(_) => 75,
            Self::OperationFailed(_) => 4,
        }
    }

    fn detail(&self) -> &str {
        match self {
            Self::ArgError(detail)
            | Self::IdentityChanged(detail)
            | Self::PermissionDenied(detail)
            | Self::Unsupported(detail)
            | Self::Rejected(detail)
            | Self::OperationFailed(detail) => detail,
        }
    }
}

#[derive(Debug)]
struct Applied {
    pid: u32,
    start_token: u64,
    operation: String,
}

#[derive(Debug, Serialize)]
struct SuccessEnvelope {
    schema: u32,
    status: &'static str,
    pid: u32,
    start_token: u64,
    operation: String,
}

#[derive(Debug, Serialize)]
struct ErrorEnvelope<'a> {
    schema: u32,
    status: &'static str,
    kind: &'static str,
    detail: &'a str,
}

fn main() -> ExitCode {
    let outcome = run();
    emit(outcome)
}

#[cfg(any(unix, windows))]
fn run() -> Result<Applied, HelperError> {
    let (pid, start_token, operation) = parse_args(std::env::args().skip(1))?;
    apply_operation(pid, start_token, &operation)?;
    Ok(Applied {
        pid,
        start_token,
        operation: operation.contract_name(),
    })
}

#[cfg(not(any(unix, windows)))]
fn run() -> Result<Applied, HelperError> {
    Err(HelperError::Unsupported(
        "foreign process control helper is Unix and Windows only".to_owned(),
    ))
}

fn parse_args(
    mut args: impl Iterator<Item = String>,
) -> Result<(u32, u64, Operation), HelperError> {
    let pid = parse_nonzero_u32(args.next(), "pid")?;
    let start_token = parse_nonzero_u64(args.next(), "start-token")?;
    let operation = args
        .next()
        .ok_or_else(|| HelperError::ArgError(usage().to_owned()))?;
    if args.next().is_some() {
        return Err(HelperError::ArgError(usage().to_owned()));
    }
    Ok((pid, start_token, Operation::parse(&operation)?))
}

fn parse_nonzero_u32(raw: Option<String>, label: &str) -> Result<u32, HelperError> {
    let value = raw
        .ok_or_else(|| HelperError::ArgError(usage().to_owned()))?
        .parse::<u32>()
        .map_err(|_| HelperError::ArgError(format!("{label} must be a positive integer")))?;
    if value == 0 {
        return Err(HelperError::ArgError(format!(
            "{label} must be a positive integer"
        )));
    }
    Ok(value)
}

fn parse_nonzero_u64(raw: Option<String>, label: &str) -> Result<u64, HelperError> {
    let value = raw
        .ok_or_else(|| HelperError::ArgError(usage().to_owned()))?
        .parse::<u64>()
        .map_err(|_| HelperError::ArgError(format!("{label} must be a positive integer")))?;
    if value == 0 {
        return Err(HelperError::ArgError(format!(
            "{label} must be a positive integer"
        )));
    }
    Ok(value)
}

const fn usage() -> &'static str {
    "usage: taskmanager-process-control-helper <pid> <start-token> <operation>"
}

#[cfg(unix)]
fn validate_start_token(pid: u32, expected: u64) -> Result<(), HelperError> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .map_err(|error| classify_io(error, format!("could not read /proc/{pid}/stat")))?;
    let actual = parse_start_token(&text).ok_or_else(|| {
        HelperError::OperationFailed("/proc stat had no valid start token".to_owned())
    })?;
    if actual == expected {
        Ok(())
    } else {
        Err(HelperError::IdentityChanged(format!(
            "process PID {pid} was reused before the privileged operation"
        )))
    }
}

#[cfg(unix)]
fn parse_start_token(text: &str) -> Option<u64> {
    let rparen = text.rfind(')')?;
    text.get(rparen + 1..)?
        .split_whitespace()
        .nth(19)?
        .parse()
        .ok()
}

#[cfg(unix)]
fn apply_operation(pid: u32, expected: u64, operation: &Operation) -> Result<(), HelperError> {
    match operation {
        // Signal delivery is the irreversible family (SIGKILL/SIGSTOP/...):
        // it pins the exact process instance with a pidfd BEFORE the token
        // is read — see `send_signal_checked`. The vocabulary below is
        // unchanged: end=SIGTERM, kill=SIGKILL, suspend=SIGSTOP,
        // resume=SIGCONT.
        Operation::End => send_signal_checked(pid, expected, SignalName::Terminate),
        Operation::Kill => send_signal_checked(pid, expected, SignalName::Kill),
        Operation::Suspend => send_signal_checked(pid, expected, SignalName::Stop),
        Operation::Resume => send_signal_checked(pid, expected, SignalName::Continue),
        Operation::Signal(signal) => send_signal_checked(pid, expected, *signal),
        // setpriority(2)/sched_setaffinity(2) have no pidfd variant. The
        // residual reuse window between the re-read and the syscall is
        // accepted because (a) the worst outcome is a niceness/CPU-mask set
        // on a recycled pid — never death — and (b) if the target merely
        // exited without reuse, the syscall itself returns ESRCH, typed as
        // IdentityChanged. The check and the syscall are kept adjacent with
        // no work in between.
        Operation::Priority(nice) => {
            validate_start_token(pid, expected)?;
            set_priority(pid, *nice)
        }
        Operation::Affinity(cpus) => {
            validate_start_token(pid, expected)?;
            set_affinity(pid, cpus)
        }
    }
}

/// Identity-checked signal delivery with zero check-to-act TOCTOU on the
/// pidfd seam.
///
/// The ordering IS the security property: `pidfd_open(pid)` first, then the
/// `/proc/<pid>/stat` token check, then the signal through the pinned handle.
/// The pidfd pins the exact process instance inside the kernel, so once it is
/// open, pid reuse (target exits, a successor takes the pid number) can never
/// redirect the signal: `pidfd_send_signal` reaches the pinned instance — or
/// fails ESRCH typed as IdentityChanged once it is dead — but never the
/// successor. The only remaining race is the target dying between
/// `pidfd_open` and the token read, which the same pinned handle resolves
/// harmlessly. A successor presenting an identical start token between open
/// and read would still not be harmed: the signal goes to the instance pinned
/// at open time, not to whatever currently owns the pid number.
#[cfg(unix)]
fn send_signal_checked(pid: u32, expected: u64, signal: SignalName) -> Result<(), HelperError> {
    #[cfg(target_os = "linux")]
    {
        match taskmanager_fd_bridge::pidfd_open(pid) {
            Ok(pidfd) => {
                validate_start_token(pid, expected)?;
                taskmanager_fd_bridge::pidfd_send_signal(&pidfd, signal_number(signal))
                    .map_err(|error| classify_pidfd_error(error, "process signal failed"))
            }
            // Linux < 5.1 has no pidfd syscalls; fall back to the legacy
            // re-read-then-signal path with its narrow audited residual
            // window (see `send_signal_legacy_checked`).
            Err(error) if taskmanager_fd_bridge::is_pidfd_unsupported(&error) => {
                send_signal_legacy_checked(pid, expected, signal)
            }
            Err(error) => Err(classify_pidfd_error(
                error,
                &format!("could not open pidfd for PID {pid}"),
            )),
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        send_signal_legacy_checked(pid, expected, signal)
    }
}

/// Legacy check-then-signal path: re-read the token immediately before
/// kill(2), with nothing in between.
///
/// Kept as (a) the ENOSYS fallback on Linux < 5.1 and (b) the only path on
/// non-Linux Unix. Residual window: the target may exit and the pid be
/// recycled between the check and the syscall — no tighter seam exists
/// without pidfd. Deterministic reuse cannot be constructed in a test, so
/// behavior tests assert the equivalent invariants (mismatched token never
/// signals; reaped target is typed IdentityChanged).
#[cfg(unix)]
fn send_signal_legacy_checked(
    pid: u32,
    expected: u64,
    signal: SignalName,
) -> Result<(), HelperError> {
    validate_start_token(pid, expected)?;
    send_signal(pid, native_signal(signal))
}

#[cfg(target_os = "linux")]
const fn signal_number(signal: SignalName) -> i32 {
    // nix's `Signal` is `repr(i32)` over the libc constants, so the cast
    // yields the per-platform signal number for `pidfd_send_signal`.
    native_signal(signal) as i32
}

#[cfg(target_os = "linux")]
fn classify_pidfd_error(error: io::Error, context: &str) -> HelperError {
    // pidfd_open/pidfd_send_signal surface the raw errno (ESRCH/EPERM/...);
    // classify through the same errno vocabulary as kill(2) so the typed
    // kinds and exit codes stay identical on both signal paths.
    match nix::errno::Errno::from_raw(error.raw_os_error().unwrap_or(0)) {
        nix::errno::Errno::EACCES | nix::errno::Errno::EPERM => {
            HelperError::PermissionDenied(format!("{context}: permission denied"))
        }
        nix::errno::Errno::ESRCH | nix::errno::Errno::ENOENT => {
            HelperError::IdentityChanged(format!("{context}: process no longer exists"))
        }
        nix::errno::Errno::ENOSYS | nix::errno::Errno::EOPNOTSUPP => {
            HelperError::Unsupported(format!("{context}: operation is unsupported"))
        }
        nix::errno::Errno::EINVAL => HelperError::Rejected(format!("{context}: invalid argument")),
        _ => HelperError::OperationFailed(format!("{context}: {error}")),
    }
}

#[cfg(unix)]
fn send_signal(pid: u32, signal: nix::sys::signal::Signal) -> Result<(), HelperError> {
    let raw_pid = i32::try_from(pid)
        .map_err(|_| HelperError::Rejected("pid exceeds the Unix pid range".to_owned()))?;
    nix::sys::signal::kill(nix::unistd::Pid::from_raw(raw_pid), signal)
        .map_err(|error| classify_nix_errno(error, "process signal failed"))
}

#[cfg(unix)]
fn set_priority(pid: u32, nice: i32) -> Result<(), HelperError> {
    let raw_pid = i32::try_from(pid)
        .map_err(|_| HelperError::Rejected("pid exceeds the Unix pid range".to_owned()))?;
    rustix::process::setpriority_process(rustix::process::Pid::from_raw(raw_pid), nice)
        .map_err(|error| classify_rustix_errno(error, "setting process priority failed"))
}

#[cfg(unix)]
fn set_affinity(pid: u32, cpus: &[u32]) -> Result<(), HelperError> {
    let raw_pid = i32::try_from(pid)
        .map_err(|_| HelperError::Rejected("pid exceeds the Unix pid range".to_owned()))?;
    let mut set = rustix::thread::CpuSet::new();
    for &cpu in cpus {
        let cpu = usize::try_from(cpu)
            .map_err(|_| HelperError::Rejected("CPU id is out of range".to_owned()))?;
        if cpu >= rustix::thread::CpuSet::MAX_CPU {
            return Err(HelperError::Rejected("CPU id is out of range".to_owned()));
        }
        set.set(cpu);
    }
    rustix::thread::sched_setaffinity(rustix::process::Pid::from_raw(raw_pid), &set)
        .map_err(|error| classify_rustix_errno(error, "setting process affinity failed"))
}

#[cfg(unix)]
const fn native_signal(signal: SignalName) -> nix::sys::signal::Signal {
    use nix::sys::signal::Signal;
    match signal {
        SignalName::Terminate => Signal::SIGTERM,
        SignalName::Kill => Signal::SIGKILL,
        SignalName::Stop => Signal::SIGSTOP,
        SignalName::Continue => Signal::SIGCONT,
        SignalName::Hangup => Signal::SIGHUP,
        SignalName::Interrupt => Signal::SIGINT,
        SignalName::User1 => Signal::SIGUSR1,
        SignalName::User2 => Signal::SIGUSR2,
    }
}

#[cfg(unix)]
fn classify_nix_errno(error: nix::errno::Errno, context: &str) -> HelperError {
    match error {
        nix::errno::Errno::EACCES | nix::errno::Errno::EPERM => {
            HelperError::PermissionDenied(format!("{context}: permission denied"))
        }
        nix::errno::Errno::ESRCH | nix::errno::Errno::ENOENT => {
            HelperError::IdentityChanged(format!("{context}: process no longer exists"))
        }
        nix::errno::Errno::ENOSYS | nix::errno::Errno::EOPNOTSUPP => {
            HelperError::Unsupported(format!("{context}: operation is unsupported"))
        }
        nix::errno::Errno::EINVAL => HelperError::Rejected(format!("{context}: invalid argument")),
        _ => HelperError::OperationFailed(format!("{context}: {error}")),
    }
}

#[cfg(unix)]
fn classify_rustix_errno(error: rustix::io::Errno, context: &str) -> HelperError {
    match error {
        rustix::io::Errno::ACCESS | rustix::io::Errno::PERM => {
            HelperError::PermissionDenied(format!("{context}: permission denied"))
        }
        rustix::io::Errno::SRCH | rustix::io::Errno::NOENT => {
            HelperError::IdentityChanged(format!("{context}: process no longer exists"))
        }
        rustix::io::Errno::NOSYS | rustix::io::Errno::NOTSUP => {
            HelperError::Unsupported(format!("{context}: operation is unsupported"))
        }
        rustix::io::Errno::INVAL => HelperError::Rejected(format!("{context}: invalid argument")),
        _ => HelperError::OperationFailed(format!("{context}: {error}")),
    }
}

#[cfg(unix)]
fn classify_io(error: io::Error, context: String) -> HelperError {
    match error.kind() {
        io::ErrorKind::NotFound => HelperError::IdentityChanged(context),
        io::ErrorKind::PermissionDenied => HelperError::PermissionDenied(context),
        _ => HelperError::OperationFailed(format!("{context}: {error}")),
    }
}

#[cfg(windows)]
fn validate_start_token(pid: u32, expected: u64) -> Result<(), HelperError> {
    let actual =
        taskmanager_windows_api::process_creation_time_100ns(pid).map_err(|err| match err {
            taskmanager_windows_api::WindowsApiError::PermissionDenied => {
                HelperError::PermissionDenied(format!(
                    "could not inspect process {pid}: permission denied"
                ))
            }
            taskmanager_windows_api::WindowsApiError::IdentityChanged
            | taskmanager_windows_api::WindowsApiError::QueryFailed => {
                HelperError::IdentityChanged(format!("process {pid} no longer exists"))
            }
            taskmanager_windows_api::WindowsApiError::Unsupported => {
                HelperError::Unsupported("process inspection unsupported".to_owned())
            }
            _ => HelperError::OperationFailed(format!("could not inspect process {pid}: {err}")),
        })?;
    if actual == expected {
        Ok(())
    } else {
        Err(HelperError::IdentityChanged(format!(
            "process PID {pid} was reused before the privileged operation"
        )))
    }
}

#[cfg(windows)]
fn apply_operation(pid: u32, expected: u64, operation: &Operation) -> Result<(), HelperError> {
    match operation {
        Operation::End | Operation::Kill => {
            taskmanager_windows_api::terminate_process_exact(pid, expected).map_err(map_win_api_err)
        }
        Operation::Priority(nice) => {
            let class = map_nice_to_win_priority(*nice);
            taskmanager_windows_api::set_process_priority_exact(pid, expected, class)
                .map_err(map_win_api_err)
        }
        Operation::Affinity(cpus) => {
            taskmanager_windows_api::set_process_affinity_exact(pid, expected, cpus)
                .map_err(map_win_api_err)
        }
        Operation::Suspend | Operation::Resume => Err(HelperError::Unsupported(
            "process suspend/resume is unsupported on Windows".to_owned(),
        )),
        Operation::Signal(signal) => match signal {
            SignalName::Terminate | SignalName::Kill => {
                taskmanager_windows_api::terminate_process_exact(pid, expected)
                    .map_err(map_win_api_err)
            }
            _ => Err(HelperError::Unsupported(format!(
                "signal {} is unsupported on Windows",
                signal.contract_name()
            ))),
        },
    }
}

#[cfg(windows)]
fn map_nice_to_win_priority(nice: i32) -> taskmanager_windows_api::ProcessPriorityClass {
    if nice <= -15 {
        taskmanager_windows_api::ProcessPriorityClass::Realtime
    } else if nice <= -6 {
        taskmanager_windows_api::ProcessPriorityClass::High
    } else if nice <= -1 {
        taskmanager_windows_api::ProcessPriorityClass::AboveNormal
    } else if nice <= 0 {
        taskmanager_windows_api::ProcessPriorityClass::Normal
    } else if nice <= 6 {
        taskmanager_windows_api::ProcessPriorityClass::BelowNormal
    } else {
        taskmanager_windows_api::ProcessPriorityClass::Idle
    }
}

#[cfg(windows)]
fn map_win_api_err(err: taskmanager_windows_api::WindowsApiError) -> HelperError {
    match err {
        taskmanager_windows_api::WindowsApiError::PermissionDenied => {
            HelperError::PermissionDenied("permission denied".to_owned())
        }
        taskmanager_windows_api::WindowsApiError::IdentityChanged => {
            HelperError::IdentityChanged("process identity changed or process exited".to_owned())
        }
        taskmanager_windows_api::WindowsApiError::InvalidInput => {
            HelperError::Rejected("invalid input argument".to_owned())
        }
        taskmanager_windows_api::WindowsApiError::Unsupported => {
            HelperError::Unsupported("operation unsupported on this system".to_owned())
        }
        _ => HelperError::OperationFailed(format!("{err}")),
    }
}

fn emit(outcome: Result<Applied, HelperError>) -> ExitCode {
    match outcome {
        Ok(applied) => {
            let envelope = SuccessEnvelope {
                schema: SCHEMA_VERSION,
                status: "applied",
                pid: applied.pid,
                start_token: applied.start_token,
                operation: applied.operation,
            };
            write_json(&envelope).map_or_else(|_| ExitCode::from(4), |_| ExitCode::SUCCESS)
        }
        Err(error) => {
            let envelope = ErrorEnvelope {
                schema: SCHEMA_VERSION,
                status: "error",
                kind: error.kind(),
                detail: error.detail(),
            };
            write_json(&envelope)
                .map_or_else(|_| ExitCode::from(4), |_| ExitCode::from(error.exit_code()))
        }
    }
}

fn write_json<T: Serialize>(value: &T) -> Result<(), ()> {
    let json = serde_json::to_string(value).map_err(|_| ())?;
    println!("{json}");
    Ok(())
}

#[cfg(test)]
#[path = "../tests/headless/process_control_helper_main.rs"]
mod tests;
