//! Terminal diagnostic report generation and export for TUI.
//!
//! The report is built and redacted by the shared application diagnostic
//! authority: [`DiagnosticBundleEngine::build`] folds the already-projected
//! facts into the neutral [`DiagnosticBundle`], and
//! [`DiagnosticBundle::generate_redacted_report`] applies the audited core
//! redaction contract. The terminal frontend only renders the returned
//! Markdown text and owns the file write; it never composes the core
//! `DiagnosticBundlePlan` itself, so there is exactly one semantic path for
//! diagnostic facts and redaction.

use std::fmt;
use std::path::{Path, PathBuf};

use taskmanager_application::i18n::t;
use taskmanager_application::{DiagnosticBundleEngine, ProjectedSystemTelemetry};
use taskmanager_core::DiagnosticBundleError;
use taskmanager_core::core::config::Config;
use taskmanager_core::core::hardware::HardwareInfo;
use taskmanager_core::core::metrics::SystemSnapshot;
use taskmanager_core::core::process::ProcessItem;
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource};

use crate::TuiApp;
use crate::ui::pages::system_data::diagnostic_failure_message;

/// Default diagnostic report export filename.
pub const DEFAULT_DIAGNOSTIC_FILENAME: &str = "taskforest-diagnostic.txt";

/// Honest terminal diagnostic export failure.
///
/// A missing fact set is never folded into a success-looking empty report and
/// a failed redaction never degrades into writing unredacted content.
#[derive(Debug)]
pub enum DiagnosticExportError {
    /// No diagnostic fact has been observed yet, so there is nothing to export.
    NoData,
    /// The shared application engine failed to render the redacted report.
    Report(DiagnosticBundleError),
    /// The rendered report could not be written to the destination.
    Io(std::io::Error),
}

impl fmt::Display for DiagnosticExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoData => formatter.write_str("no diagnostic data has been observed"),
            Self::Report(error) => {
                write!(formatter, "diagnostic report generation failed: {error}")
            }
            Self::Io(error) => write!(formatter, "diagnostic report write failed: {error}"),
        }
    }
}

impl std::error::Error for DiagnosticExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NoData => None,
            Self::Report(error) => Some(error),
            Self::Io(error) => Some(error),
        }
    }
}

/// Build and render the redacted Markdown diagnostic report through the shared
/// application diagnostic engine.
///
/// `Ok(None)` means no diagnostic fact has been observed yet; the export path
/// answers that with a typed "no data" warning instead of writing a
/// success-looking empty report.
pub fn render_diagnostic_report(
    hardware: Option<&HardwareInfo>,
    snapshot: Option<&SystemSnapshot>,
    telemetry: Option<&ProjectedSystemTelemetry>,
    config: Option<&Config>,
    processes: &[ProcessItem],
    usernames: impl IntoIterator<Item = String>,
) -> Result<Option<String>, DiagnosticBundleError> {
    if hardware.is_none() && snapshot.is_none() && telemetry.is_none() && processes.is_empty() {
        return Ok(None);
    }
    let bundle =
        DiagnosticBundleEngine::new().build(hardware, snapshot, telemetry, None, config, processes);
    bundle.generate_redacted_report(usernames).map(Some)
}

/// Resolve the default diagnostic export destination path.
/// Prefers `app.export_dir/taskforest-diagnostic.txt` if configured;
/// otherwise defaults to `/tmp/taskforest-diagnostic.txt` (or system temp directory).
#[must_use]
pub fn default_diagnostic_path(export_dir: Option<&Path>) -> PathBuf {
    export_dir.map_or_else(
        || {
            let tmp = PathBuf::from("/tmp");
            if tmp.is_dir() {
                tmp.join(DEFAULT_DIAGNOSTIC_FILENAME)
            } else {
                std::env::temp_dir().join(DEFAULT_DIAGNOSTIC_FILENAME)
            }
        },
        |dir| dir.join(DEFAULT_DIAGNOSTIC_FILENAME),
    )
}

impl TuiApp {
    /// Render the redacted terminal diagnostic report from the current
    /// projection through the shared application engine.
    ///
    /// `Ok(None)` means the projection carries no diagnostic fact yet.
    pub fn format_diagnostic_report(&self) -> Result<Option<String>, DiagnosticBundleError> {
        render_diagnostic_report(
            self.projection().hardware.as_ref(),
            self.projection().snapshot.as_ref(),
            self.projection().system_telemetry.as_ref(),
            Some(&self.config_draft),
            self.projection().processes_slice(),
            std::iter::empty(),
        )
    }

    /// Export the formatted diagnostic summary to `path` and report footer
    /// feedback. A missing fact set or a write failure produces an honest
    /// warning/error and never a fabricated success.
    pub fn export_diagnostic_report_to(
        &mut self,
        path: impl Into<PathBuf>,
    ) -> Result<PathBuf, DiagnosticExportError> {
        let destination = path.into();
        let report = match self.format_diagnostic_report() {
            Ok(Some(report)) => report,
            Ok(None) => {
                self.report_notice(
                    FeedbackSource::Persistence,
                    FeedbackSeverity::Warning,
                    FeedbackLifecycle::SHORT,
                    t("system.export_no_data"),
                );
                return Err(DiagnosticExportError::NoData);
            }
            Err(error) => {
                let msg = diagnostic_failure_message(&error);
                self.report_notice(
                    FeedbackSource::Persistence,
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::UntilReplaced,
                    msg,
                );
                return Err(DiagnosticExportError::Report(error));
            }
        };
        match std::fs::write(&destination, report) {
            Ok(()) => {
                let msg =
                    t("diagnostics.complete").replace("{path}", &destination.display().to_string());
                self.report_notice(
                    FeedbackSource::Persistence,
                    FeedbackSeverity::Success,
                    FeedbackLifecycle::SHORT,
                    msg,
                );
                Ok(destination)
            }
            Err(err) => {
                let msg =
                    t("diagnostics.failed_detail").replace("{reason}", t("diagnostics.failure_io"));
                self.report_notice(
                    FeedbackSource::Persistence,
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::UntilReplaced,
                    msg,
                );
                Err(DiagnosticExportError::Io(err))
            }
        }
    }

    /// Export the formatted diagnostic summary report to `export_dir/taskforest-diagnostic.txt`
    /// or `/tmp/taskforest-diagnostic.txt`.
    pub fn export_diagnostic_report(&mut self) -> Result<PathBuf, DiagnosticExportError> {
        let destination = default_diagnostic_path(self.export_dir.as_deref());
        self.export_diagnostic_report_to(destination)
    }
}

#[cfg(test)]
#[path = "../tests/headless/diagnostic_report_tests.rs"]
mod tests;
