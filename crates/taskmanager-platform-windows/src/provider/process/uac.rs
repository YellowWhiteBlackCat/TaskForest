//! ADR-035 stage 2: the production Windows UAC (runas) transport for
//! foreign-process control.
//!
//! This is the Windows counterpart of the Linux pkexec crossing in
//! `taskmanager-escalation::polkit::process_control`. The driver owns the
//! per-call mechanics: it proves an interactive session exists (Session 0
//! cannot show a consent), pre-creates the one-shot, randomly named reply
//! file, builds the fixed helper command line with the escalation crate's
//! PURE builder, and drives the audited `runas` call group in
//! `taskmanager-windows-api` (`ShellExecuteExW("runas")` +
//! `SEE_MASK_NOCLOSEPROCESS`, bounded `WaitForSingleObject`,
//! `GetExitCodeProcess`, `CloseHandle`). Every raw result is folded into the
//! typed [`UacCrossingObservation`] vocabulary and classified by
//! `taskmanager-escalation::uac`, so this file never invents an outcome.
//!
//! The helper binary is the same `taskmanager-process-control-helper` with
//! the same PID + creation-token + operation arguments plus the reply-file
//! channel; it re-validates the kernel creation time on its own elevated
//! handle, so a PID reuse can never inherit the user's intent.

#[cfg(any(windows, test))]
use std::path::Path;
#[cfg(windows)]
use std::path::PathBuf;
#[cfg(windows)]
use std::time::Duration;

#[cfg(windows)]
use taskmanager_escalation::polkit::{ForeignProcessControlOperation, ForeignProcessControlTarget};
#[cfg(any(windows, test))]
use taskmanager_escalation::uac::UacCrossingObservation;
#[cfg(windows)]
use taskmanager_escalation::uac::{
    UacForeignProcessControlTransport, reply_channel_file_name, runas_command_line,
};
#[cfg(any(windows, test))]
use taskmanager_windows_api::RunasLaunchOutcome;

/// The packaged helper binary name (the crate's `[[bin]]` name on Windows).
#[cfg(windows)]
const HELPER_BINARY_NAME: &str = "taskmanager-process-control-helper.exe";

/// The bounded wall for one crossing, mirroring the Linux interactive
/// pkexec deadline: an abandoned consent must never park the caller forever.
#[cfg(windows)]
const RUNAS_DEADLINE: Duration = Duration::from_secs(120);

/// The largest reply payload the driver will read back (bytes). A larger
/// helper reply is truncated and therefore fails contract parsing — it can
/// never balloon memory or masquerade as success.
#[cfg(any(windows, test))]
const MAX_REPLY_BYTES: usize = 64 * 1024;

/// Map the boundary's raw launch result plus the reply-channel bytes into
/// the typed transport-fact vocabulary.
///
/// Pure over its inputs (the reply bytes are supplied by a closure so the
/// file read happens only on the completed path); fixture-provable on any
/// host. The completed-but-unreadable case deliberately becomes an EMPTY
/// reply — the shared contract reader then classifies it as a protocol
/// violation, never success.
#[cfg(any(windows, test))]
pub(crate) fn map_runas_launch(
    launch: RunasLaunchOutcome,
    reply: impl FnOnce() -> Option<Vec<u8>>,
) -> UacCrossingObservation {
    match launch {
        RunasLaunchOutcome::Completed { .. } => UacCrossingObservation::HelperReply {
            payload: reply().unwrap_or_default(),
        },
        RunasLaunchOutcome::LaunchFailed { win32_error } => {
            UacCrossingObservation::LaunchFailed { win32_error }
        }
        RunasLaunchOutcome::DeadlineExceeded => UacCrossingObservation::DeadlineExceeded,
        RunasLaunchOutcome::Unsupported => UacCrossingObservation::TransportUnwired,
    }
}

/// Read the one-shot reply payload, capped at [`MAX_REPLY_BYTES`].
#[cfg(any(windows, test))]
pub(crate) fn read_reply_bounded(path: &Path) -> Option<Vec<u8>> {
    use std::io::Read;

    let file = std::fs::File::open(path).ok()?;
    let mut payload = Vec::new();
    file.take(MAX_REPLY_BYTES as u64)
        .read_to_end(&mut payload)
        .ok()?;
    Some(payload)
}

/// Pre-create the per-call, randomly named reply file (ADR-035 decision 4).
///
/// Exclusive `create_new` under the per-user temp directory is the channel's
/// access discipline: the name is drawn from a process-addressed `RandomState`
/// hash so two crossings never collide, and a name that somehow exists is
/// retried a bounded number of times before the crossing reports
/// [`UacCrossingObservation::ReplyChannelUnavailable`]. The elevated helper
/// only ever truncates and rewrites this file the unprivileged app created.
#[cfg(windows)]
fn create_reply_channel() -> Option<PathBuf> {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};

    static CROSSING_SEQUENCE: AtomicU64 = AtomicU64::new(0);
    const MAX_NAME_ATTEMPTS: u32 = 4;

    let mut attempts = 0_u32;
    loop {
        let sequence = CROSSING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u64(sequence);
        hasher.write_u32(std::process::id());
        let path = std::env::temp_dir().join(reply_channel_file_name(hasher.finish()));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => return Some(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                attempts += 1;
                if attempts >= MAX_NAME_ATTEMPTS {
                    return None;
                }
            }
            Err(_) => return None,
        }
    }
}

/// The packaged helper location: a sidecar binary next to the running app.
///
/// The Windows MSI does not install this helper yet (ADR-035 current-release
/// boundary), so on today's boxes the launch fails with `ERROR_FILE_NOT_FOUND`
/// and maps to the typed `HelperUnavailable` — the honest missing-install
/// answer, never a fabricated crossing.
#[cfg(windows)]
fn packaged_helper_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_default()
        .join(HELPER_BINARY_NAME)
}

/// The production runas transport (ADR-035 stage 2).
#[cfg(windows)]
pub(crate) struct RunasUacTransport {
    helper: PathBuf,
}

#[cfg(windows)]
impl RunasUacTransport {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            helper: packaged_helper_path(),
        }
    }
}

#[cfg(windows)]
impl Default for RunasUacTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(windows)]
impl UacForeignProcessControlTransport for RunasUacTransport {
    fn cross(
        &self,
        target: ForeignProcessControlTarget,
        operation: &ForeignProcessControlOperation,
    ) -> UacCrossingObservation {
        // A UAC consent needs an interactive session; Session 0 (services)
        // cannot show one, and a failed session query is equally
        // unattributable — neither is a user refusal.
        if !matches!(
            taskmanager_windows_api::interactive_session_available(),
            Ok(true)
        ) {
            return UacCrossingObservation::ConsentUnavailable;
        }
        let Some(channel) = create_reply_channel() else {
            return UacCrossingObservation::ReplyChannelUnavailable;
        };
        let parameters = runas_command_line(target, operation, &channel);
        let launch = taskmanager_windows_api::run_elevated_and_wait(
            &self.helper.to_string_lossy(),
            &parameters,
            RUNAS_DEADLINE,
        );
        let observation = map_runas_launch(launch, || read_reply_bounded(&channel));
        // The channel is one-shot: remove it regardless of outcome so no
        // stale reply can ever survive a crossing.
        let _ = std::fs::remove_file(&channel);
        observation
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/platform_windows_provider_process_uac.rs"]
mod tests;
