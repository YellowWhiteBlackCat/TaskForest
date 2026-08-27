use serde::{Deserialize, Serialize};

use super::domain::{SystemDomainValue, SystemObservationState};
use crate::core::{FailureKind, MemoryMetrics, SourceStatus};

/// Memory facts and source truth on the memory sampler's own cadence.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryTelemetryObservation {
    value: SystemDomainValue<MemoryMetrics>,
}

impl MemoryTelemetryObservation {
    #[must_use]
    pub fn current(value: MemoryMetrics, observed_at_ms: u64, sources: Vec<SourceStatus>) -> Self {
        Self {
            value: SystemDomainValue::current(value, observed_at_ms, sources),
        }
    }

    #[must_use]
    pub fn partial(
        value: MemoryMetrics,
        observed_at_ms: u64,
        failure: FailureKind,
        sources: Vec<SourceStatus>,
    ) -> Self {
        Self {
            value: SystemDomainValue::partial(value, observed_at_ms, failure, sources),
        }
    }

    #[must_use]
    pub fn stale(
        last_value: MemoryMetrics,
        last_success_ms: u64,
        failure: FailureKind,
        sources: Vec<SourceStatus>,
    ) -> Self {
        Self {
            value: SystemDomainValue::stale(last_value, last_success_ms, failure, sources),
        }
    }

    #[must_use]
    pub fn unavailable(failure: FailureKind, sources: Vec<SourceStatus>) -> Self {
        Self {
            value: SystemDomainValue::unavailable(failure, sources),
        }
    }

    #[must_use]
    pub const fn state(&self) -> SystemObservationState {
        self.value.state()
    }

    #[must_use]
    pub const fn current_value(&self) -> Option<&MemoryMetrics> {
        self.value.current_value()
    }

    #[must_use]
    pub const fn last_known_value(&self) -> Option<&MemoryMetrics> {
        self.value.last_known_value()
    }

    #[must_use]
    pub fn sources(&self) -> &[SourceStatus] {
        self.value.sources()
    }
}
