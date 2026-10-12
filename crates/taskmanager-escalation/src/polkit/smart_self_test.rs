//! Polkit crossing for the fixed SMART self-test helper.

#![forbid(unsafe_code)]

use std::io;

use crate::EscalationDenialReason;

use super::json_reader::{Json, JsonReader};
use super::{HelperOutput, classify_pkexec_no_contract};
#[cfg(target_os = "linux")]
use super::{INTERACTIVE_PKEXEC_DEADLINE, run_bounded};

/// The executable annotated by the SMART self-test polkit action.
#[cfg(target_os = "linux")]
pub(crate) const SMART_SELF_TEST_HELPER_PATH: &str =
    "/usr/libexec/taskforest-smart-self-test-helper";

/// Fixed request fields passed as separate argv items, never as shell text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SmartSelfTestHelperRequest<'a> {
    /// Stable persistent disk identity from the current storage projection.
    pub device_id: &'a str,
    /// Current lifecycle generation at confirmation time.
    pub device_generation: u64,
    /// Opaque block-device locator, usually a whole-device kernel name.
    pub locator: &'a str,
    /// Core storage protocol in snake case.
    pub protocol: &'a str,
    /// Core storage interconnect in snake case.
    pub interconnect: &'a str,
    /// Exact self-test kind in snake case.
    pub kind: &'a str,
}

/// Helper-owned failure after the OS authorization process launched it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmartSelfTestHelperFailure {
    /// The request was malformed or outside the fixed operation contract.
    InvalidTarget,
    /// Live sysfs identity no longer matches the confirmed target.
    IdentityChanged,
    /// smartctl is absent from the helper's execution environment.
    MissingTool,
    /// The helper still cannot access the target after authorization.
    PermissionDenied,
    /// The helper could not verify support for the exact requested test kind.
    SupportUnknown,
    /// smartctl did not finish before its operation deadline.
    TimedOut,
    /// The selected device family rejects SMART self-test operations.
    Unsupported,
    /// The device or provider rejected the requested operation.
    Rejected,
    /// The provider could not return a trustworthy result.
    ProviderUnavailable,
}

/// Result of invoking the SMART self-test helper through pkexec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SmartSelfTestHelperOutcome {
    /// The helper verified target identity and started the requested test.
    Started,
    /// The helper launched but returned a typed operation failure.
    HelperError(SmartSelfTestHelperFailure),
    /// Authorization could not reach a valid helper contract.
    Unavailable {
        /// Typed reason the authorization crossing did not complete.
        reason: EscalationDenialReason,
        /// Bounded diagnostic detail.
        detail: String,
    },
}

/// Injectable process seam for pkexec. Tests return fixed output and never
/// launch a real helper or authorization prompt.
pub trait SmartSelfTestHelperProcess {
    /// Run the exact fixed helper with one target request.
    fn run(&self, request: SmartSelfTestHelperRequest<'_>) -> io::Result<HelperOutput>;
}

/// Production driver for one SMART self-test operation through pkexec.
#[derive(Debug, Clone, Copy, Default)]
pub struct PkexecSmartSelfTestHelper;

impl PkexecSmartSelfTestHelper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[cfg(target_os = "linux")]
impl SmartSelfTestHelperProcess for PkexecSmartSelfTestHelper {
    fn run(&self, request: SmartSelfTestHelperRequest<'_>) -> io::Result<HelperOutput> {
        use std::process::{Command, Stdio};

        let mut command = Command::new("pkexec");
        command
            .arg(SMART_SELF_TEST_HELPER_PATH)
            .arg("--start-self-test")
            .arg(request.device_id)
            .arg(request.device_generation.to_string())
            .arg(request.locator)
            .arg(request.protocol)
            .arg(request.interconnect)
            .arg(request.kind)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let output = run_bounded(&mut command, INTERACTIVE_PKEXEC_DEADLINE)
            .map_err(|error| error.into_io_error("the pkexec SMART self-test helper"))?;
        Ok(HelperOutput {
            status_code: output.status_code,
            stdout: output.stdout,
        })
    }
}

#[cfg(not(target_os = "linux"))]
impl SmartSelfTestHelperProcess for PkexecSmartSelfTestHelper {
    fn run(&self, _request: SmartSelfTestHelperRequest<'_>) -> io::Result<HelperOutput> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "polkit SMART self-tests are only available on Linux",
        ))
    }
}

/// Invoke a SMART self-test through an injectable process seam.
pub fn invoke_smart_self_test_helper_with<P: SmartSelfTestHelperProcess>(
    process: &P,
    request: SmartSelfTestHelperRequest<'_>,
) -> SmartSelfTestHelperOutcome {
    let output = match process.run(request) {
        Ok(output) => output,
        Err(error) => {
            let reason = if error.kind() == io::ErrorKind::Unsupported {
                EscalationDenialReason::Unsupported
            } else if error.kind() == io::ErrorKind::TimedOut {
                EscalationDenialReason::AuthorizationUnavailable
            } else {
                EscalationDenialReason::HelperUnavailable
            };
            return SmartSelfTestHelperOutcome::Unavailable {
                reason,
                detail: format!("could not complete the SMART self-test helper crossing: {error}"),
            };
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    match parse_helper_output(&stdout) {
        ParsedOutput::Started if output.status_code == Some(0) => {
            SmartSelfTestHelperOutcome::Started
        }
        ParsedOutput::HelperError(failure) => SmartSelfTestHelperOutcome::HelperError(failure),
        ParsedOutput::Started | ParsedOutput::NotContract => {
            let (reason, detail) = classify_pkexec_no_contract(
                output.status_code,
                smart_self_test_helper_path_for_detail(),
                &stdout,
            );
            SmartSelfTestHelperOutcome::Unavailable { reason, detail }
        }
    }
}

/// Invoke the production pkexec SMART self-test crossing.
#[must_use]
pub fn invoke_smart_self_test_helper(
    request: SmartSelfTestHelperRequest<'_>,
) -> SmartSelfTestHelperOutcome {
    invoke_smart_self_test_helper_with(&PkexecSmartSelfTestHelper, request)
}

enum ParsedOutput {
    Started,
    HelperError(SmartSelfTestHelperFailure),
    NotContract,
}

fn parse_helper_output(stdout: &str) -> ParsedOutput {
    let Ok(json) = JsonReader::parse(stdout) else {
        return ParsedOutput::NotContract;
    };
    if integer_field(&json, "schema") != Some(1) {
        return ParsedOutput::NotContract;
    }
    match json.get("status").and_then(Json::as_str) {
        Some("started") if json.get("kind").is_none() => ParsedOutput::Started,
        Some("error") => json
            .get("kind")
            .and_then(Json::as_str)
            .and_then(SmartSelfTestHelperFailure::from_contract_str)
            .map_or(ParsedOutput::NotContract, ParsedOutput::HelperError),
        _ => ParsedOutput::NotContract,
    }
}

fn integer_field(json: &Json, key: &str) -> Option<i64> {
    match json.get(key)? {
        Json::Number(value) if value.is_finite() && value.fract() == 0.0 => {
            i64::try_from(*value as i128).ok()
        }
        _ => None,
    }
}

impl SmartSelfTestHelperFailure {
    fn from_contract_str(value: &str) -> Option<Self> {
        Some(match value {
            "invalid_target" => Self::InvalidTarget,
            "identity_changed" => Self::IdentityChanged,
            "missing_tool" => Self::MissingTool,
            "permission_denied" => Self::PermissionDenied,
            "support_unknown" => Self::SupportUnknown,
            "timed_out" => Self::TimedOut,
            "unsupported" => Self::Unsupported,
            "rejected" => Self::Rejected,
            "provider_unavailable" => Self::ProviderUnavailable,
            _ => return None,
        })
    }
}

#[cfg(target_os = "linux")]
fn smart_self_test_helper_path_for_detail() -> &'static str {
    SMART_SELF_TEST_HELPER_PATH
}

#[cfg(not(target_os = "linux"))]
fn smart_self_test_helper_path_for_detail() -> &'static str {
    "the annotated SMART self-test helper path"
}

#[cfg(test)]
#[path = "../../tests/headless/polkit_smart_self_test_tests.rs"]
mod tests;
