//! Fixed-argument polkit crossing for the Mission Center-compatible First Run
//! setup asset.
//!
//! This module is deliberately separate from the GPU perf helper parser. It
//! owns only the install/revert action vocabulary, pkexec process seam, and
//! bounded typed failure mapping for the packaged setup helper.

use std::io;

#[cfg(target_os = "linux")]
const SETUP_HELPER_PATH: &str = "/usr/libexec/taskmanager-setup-helper";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupScriptOperation {
    Install,
    Revert,
}

impl SetupScriptOperation {
    #[must_use]
    pub const fn argument(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Revert => "revert",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupScriptFailure {
    PermissionDenied,
    HelperUnavailable,
    MissingDependency,
    Rejected,
    ProviderFault,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupScriptOutcome {
    Success,
    Failed {
        kind: SetupScriptFailure,
        detail: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupScriptProcessOutput {
    pub status_code: Option<i32>,
    pub stderr: Vec<u8>,
}

pub trait SetupScriptProcess {
    fn run(&self, operation: SetupScriptOperation) -> io::Result<SetupScriptProcessOutput>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PkexecSetupScript;

impl PkexecSetupScript {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[cfg(target_os = "linux")]
impl SetupScriptProcess for PkexecSetupScript {
    fn run(&self, operation: SetupScriptOperation) -> io::Result<SetupScriptProcessOutput> {
        use std::process::{Command, Stdio};

        let mut command = Command::new("pkexec");
        command
            .arg(SETUP_HELPER_PATH)
            .arg(operation.argument())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        // Bounded run: the first-run prompt may wait for a human, but not
        // forever; a runaway stderr diagnostic is capped instead of buffered.
        let output = super::bounded_runner::run_bounded(
            &mut command,
            super::bounded_runner::INTERACTIVE_PKEXEC_DEADLINE,
        )
        .map_err(|error| error.into_io_error("the pkexec setup helper"))?;
        Ok(SetupScriptProcessOutput {
            status_code: output.status_code,
            stderr: output.stderr,
        })
    }
}

#[cfg(not(target_os = "linux"))]
impl SetupScriptProcess for PkexecSetupScript {
    fn run(&self, _operation: SetupScriptOperation) -> io::Result<SetupScriptProcessOutput> {
        Ok(SetupScriptProcessOutput {
            status_code: None,
            stderr: b"first-run setup is Linux-only".to_vec(),
        })
    }
}

#[must_use]
pub fn invoke_setup_script(operation: SetupScriptOperation) -> SetupScriptOutcome {
    invoke_setup_script_with(&PkexecSetupScript::new(), operation)
}

#[must_use]
pub fn invoke_setup_script_with<P: SetupScriptProcess>(
    process: &P,
    operation: SetupScriptOperation,
) -> SetupScriptOutcome {
    let output = match process.run(operation) {
        Ok(output) => output,
        Err(error) => {
            let detail = if error.kind() == io::ErrorKind::TimedOut {
                format!("the pkexec setup helper crossing was killed at its deadline: {error}")
            } else {
                format!("could not invoke pkexec setup helper: {error}")
            };
            return SetupScriptOutcome::Failed {
                kind: SetupScriptFailure::HelperUnavailable,
                detail,
            };
        }
    };
    if output.status_code == Some(0) {
        return SetupScriptOutcome::Success;
    }

    let detail = bounded_process_detail(&output.stderr);
    let kind = match output.status_code {
        Some(64 | 10 | 75) => SetupScriptFailure::Rejected,
        // 11 is retained for the historical pkexec wrapper contract; 69 is
        // the helper's EX_UNAVAILABLE value for a missing udevadm dependency.
        Some(11 | 69) => SetupScriptFailure::MissingDependency,
        Some(74) => SetupScriptFailure::ProviderFault,
        Some(126 | 127) | None => SetupScriptFailure::HelperUnavailable,
        _ => SetupScriptFailure::PermissionDenied,
    };
    SetupScriptOutcome::Failed { kind, detail }
}

fn bounded_process_detail(stderr: &[u8]) -> String {
    const MAX_DETAIL_BYTES: usize = 512;
    let bounded = &stderr[..stderr.len().min(MAX_DETAIL_BYTES)];
    // `from_utf8_lossy` may expand invalid bytes into multi-byte replacement
    // characters, so the shared char-boundary cut is still needed afterwards.
    let lossy = String::from_utf8_lossy(bounded);
    let detail = super::truncate_at_char_boundary(lossy.trim(), MAX_DETAIL_BYTES);
    if detail.is_empty() {
        "setup helper exited without a diagnostic".to_owned()
    } else {
        detail
    }
}

#[cfg(test)]
#[path = "../../tests/headless/escalation_polkit_setup.rs"]
mod tests;
