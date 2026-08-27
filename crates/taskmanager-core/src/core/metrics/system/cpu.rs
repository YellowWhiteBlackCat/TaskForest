use serde::{Deserialize, Serialize};

use super::domain::{SystemDomainValue, SystemObservationState};
use crate::core::{CpuMetrics, FailureKind, SourceStatus};

/// CPU facts and source truth on the CPU sampler's own cadence.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CpuTelemetryObservation {
    value: SystemDomainValue<CpuMetrics>,
}

impl CpuTelemetryObservation {
    #[must_use]
    pub fn current(value: CpuMetrics, observed_at_ms: u64, sources: Vec<SourceStatus>) -> Self {
        Self {
            value: SystemDomainValue::current(value, observed_at_ms, sources),
        }
    }

    #[must_use]
    pub fn partial(
        value: CpuMetrics,
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
        last_value: CpuMetrics,
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
    pub const fn current_value(&self) -> Option<&CpuMetrics> {
        self.value.current_value()
    }

    #[must_use]
    pub const fn last_known_value(&self) -> Option<&CpuMetrics> {
        self.value.last_known_value()
    }

    #[must_use]
    pub fn sources(&self) -> &[SourceStatus] {
        self.value.sources()
    }
}
