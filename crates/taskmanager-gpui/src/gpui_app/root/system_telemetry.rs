//! Headless policy for consuming application-correlated system telemetry.
//!
//! GPUI owns only the latest typed projection and the independently renderable
//! snapshot.
//! Collection, revision correlation, and provider selection remain
//! outside the toolkit.

use taskmanager_application::{CorrelatedSystemTelemetryOutcome, SystemTelemetryDomain};
use taskmanager_telemetry_store::{CorrelatedIngestionReport, CorrelatedSystemTelemetryIngestor};

pub(super) type SystemHistoryIngestionError = taskmanager_shell::history::HistoryIngestionError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SystemHistoryIngestionDiagnostic {
    pub(super) revision: taskmanager_application::SystemTelemetryRevision,
    pub(super) domain: SystemTelemetryDomain,
    pub(super) error: SystemHistoryIngestionError,
}

/// Append exactly one application-correlated domain outcome.
///
/// Raw runtime events and projections cannot call this boundary. Observed
/// outcomes retain their typed sampling time; accepted failures advance the
/// selected domain with an explicit gap.
pub(super) fn ingest_correlated_system_outcome(
    ingestor: &CorrelatedSystemTelemetryIngestor,
    correlated: &CorrelatedSystemTelemetryOutcome,
) -> Result<CorrelatedIngestionReport, SystemHistoryIngestionError> {
    taskmanager_shell::history::ingest_correlated_system_outcome(ingestor, correlated)
}

pub(super) fn record_history_ingestion_error(
    diagnostics: &mut Vec<SystemHistoryIngestionDiagnostic>,
    correlated: &CorrelatedSystemTelemetryOutcome,
    error: SystemHistoryIngestionError,
) {
    const DIAGNOSTIC_CAPACITY: usize = 32;
    diagnostics.push(SystemHistoryIngestionDiagnostic {
        revision: correlated.event.revision(),
        domain: correlated.event.domain(),
        error,
    });
    if diagnostics.len() > DIAGNOSTIC_CAPACITY {
        let remove = diagnostics.len() - DIAGNOSTIC_CAPACITY;
        diagnostics.drain(..remove);
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_system_telemetry_tests.rs"]
mod tests;
