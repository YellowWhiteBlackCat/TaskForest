//! Shared live-history composition and application-event ingestion adapter.
//!
//! The data mechanism and renderer-neutral projection live in
//! `taskmanager-telemetry-store`; this module maps application-correlated
//! outcomes onto its deliberately separate write capability.

use taskmanager_application::{
    CorrelatedPowerSupplyEvent, CorrelatedSensorEvent, CorrelatedSystemTelemetryOutcome,
    PowerSupplyEvent, SensorEvent, SystemTelemetryDomain, SystemTelemetryDomainEvent,
    SystemTelemetryDomainOutcome, SystemTelemetryUnavailable,
};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_platform_contract::SubmissionErrorKind;
use taskmanager_telemetry_store::{
    CorrelatedIngestionError, CorrelatedIngestionReport, CorrelatedSystemTelemetryIngestor,
    CorrelatedTelemetryStamp, DynamicIngestionError, DynamicIngestionReport, SystemHistoryDomain,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryIngestionError {
    InvalidZeroRevision,
    Store(CorrelatedIngestionError),
    DynamicStore(DynamicIngestionError),
}

/// Append one already-correlated sensor publication through the history
/// authority used by the active frontend session.
pub fn ingest_correlated_sensor_event(
    ingestor: &CorrelatedSystemTelemetryIngestor,
    correlated: &CorrelatedSensorEvent,
) -> Result<DynamicIngestionReport, HistoryIngestionError> {
    let stamp = CorrelatedTelemetryStamp::from_accepted_event(
        correlated.sequence.get(),
        correlated.observed_at_ms,
    )
    .ok_or(HistoryIngestionError::InvalidZeroRevision)?;
    let SensorEvent::Snapshot(snapshot) = &correlated.event;
    ingestor
        .ingest_correlated_sensors(stamp, &snapshot.value)
        .map_err(Into::into)
}

/// Append one already-correlated power-supply publication.
pub fn ingest_correlated_power_supply_event(
    ingestor: &CorrelatedSystemTelemetryIngestor,
    correlated: &CorrelatedPowerSupplyEvent,
) -> Result<DynamicIngestionReport, HistoryIngestionError> {
    let stamp = CorrelatedTelemetryStamp::from_accepted_event(
        correlated.sequence.get(),
        correlated.observed_at_ms,
    )
    .ok_or(HistoryIngestionError::InvalidZeroRevision)?;
    let PowerSupplyEvent::Snapshot(snapshot) = &correlated.event;
    ingestor
        .ingest_correlated_power_supplies(stamp, &snapshot.value)
        .map_err(Into::into)
}

impl From<CorrelatedIngestionError> for HistoryIngestionError {
    fn from(error: CorrelatedIngestionError) -> Self {
        Self::Store(error)
    }
}

impl From<DynamicIngestionError> for HistoryIngestionError {
    fn from(error: DynamicIngestionError) -> Self {
        Self::DynamicStore(error)
    }
}

/// Append one event already accepted by application correlation.
pub fn ingest_correlated_system_outcome(
    ingestor: &CorrelatedSystemTelemetryIngestor,
    correlated: &CorrelatedSystemTelemetryOutcome,
) -> Result<CorrelatedIngestionReport, HistoryIngestionError> {
    let stamp = CorrelatedTelemetryStamp::from_accepted_event(
        correlated.event.revision().get(),
        correlated.observed_at_ms,
    )
    .ok_or(HistoryIngestionError::InvalidZeroRevision)?;
    match &correlated.event {
        SystemTelemetryDomainOutcome::Observed(event) => match event {
            SystemTelemetryDomainEvent::Host { observation, .. } => {
                ingestor.ingest_correlated_host(stamp, observation)
            }
            SystemTelemetryDomainEvent::Cpu { observation, .. } => {
                ingestor.ingest_correlated_cpu(stamp, observation)
            }
            SystemTelemetryDomainEvent::Memory { observation, .. } => {
                ingestor.ingest_correlated_memory(stamp, observation)
            }
            SystemTelemetryDomainEvent::Storage { observation, .. } => {
                ingestor.ingest_correlated_storage(stamp, observation)
            }
            SystemTelemetryDomainEvent::Network { observation, .. } => {
                ingestor.ingest_correlated_network(stamp, observation)
            }
            SystemTelemetryDomainEvent::Gpu { observation, .. } => {
                ingestor.ingest_correlated_gpu(stamp, observation)
            }
        },
        SystemTelemetryDomainOutcome::Unavailable { domain, reason, .. } => ingestor
            .ingest_correlated_unavailable(
                stamp,
                history_domain(*domain),
                unavailable_failure(*reason),
            ),
    }
    .map_err(Into::into)
}

const fn history_domain(domain: SystemTelemetryDomain) -> SystemHistoryDomain {
    match domain {
        SystemTelemetryDomain::Host => SystemHistoryDomain::Host,
        SystemTelemetryDomain::Cpu => SystemHistoryDomain::Cpu,
        SystemTelemetryDomain::Memory => SystemHistoryDomain::Memory,
        SystemTelemetryDomain::Storage => SystemHistoryDomain::Storage,
        SystemTelemetryDomain::Network => SystemHistoryDomain::Network,
        SystemTelemetryDomain::Gpu => SystemHistoryDomain::Gpu,
    }
}

const fn unavailable_failure(reason: SystemTelemetryUnavailable) -> FailureKind {
    match reason {
        SystemTelemetryUnavailable::Provider(failure) => failure,
        SystemTelemetryUnavailable::Submission(SubmissionErrorKind::Busy)
        | SystemTelemetryUnavailable::Submission(SubmissionErrorKind::RuntimeStopped) => {
            FailureKind::TemporarilyUnavailable
        }
        SystemTelemetryUnavailable::Submission(SubmissionErrorKind::InvalidRequest) => {
            FailureKind::Rejected
        }
        SystemTelemetryUnavailable::Submission(SubmissionErrorKind::UnsupportedCapability) => {
            FailureKind::Unsupported
        }
    }
}
