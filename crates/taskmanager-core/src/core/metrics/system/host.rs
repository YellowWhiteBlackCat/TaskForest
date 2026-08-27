//! Host-runtime system telemetry observation: `HostRuntimeFacts` (uptime,
//! process, and thread scalars with independent freshness) and the
//! `HostRuntimeObservation` domain wrapper that carries it.

use serde::{Deserialize, Serialize};

use super::domain::{SystemDomainValue, SystemObservationState};
use crate::core::{FailureKind, ScalarObservation, SourceStatus};

/// Independently fallible host-runtime counters.
///
/// Every scalar keeps its own freshness so a missing process count cannot
/// become a believable zero merely because uptime was observed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HostRuntimeFacts {
    pub uptime_secs: ScalarObservation<u64>,
    pub processes: ScalarObservation<u64>,
    pub threads: ScalarObservation<u64>,
}

/// One independently scheduled host-runtime observation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HostRuntimeObservation {
    value: SystemDomainValue<HostRuntimeFacts>,
}

impl HostRuntimeObservation {
    #[must_use]
    pub fn current(
        value: HostRuntimeFacts,
        observed_at_ms: u64,
        sources: Vec<SourceStatus>,
    ) -> Self {
        Self {
            value: SystemDomainValue::current(value, observed_at_ms, sources),
        }
    }

    #[must_use]
    pub fn partial(
        value: HostRuntimeFacts,
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
        last_value: HostRuntimeFacts,
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
    pub const fn current_value(&self) -> Option<&HostRuntimeFacts> {
        self.value.current_value()
    }

    #[must_use]
    pub const fn last_known_value(&self) -> Option<&HostRuntimeFacts> {
        self.value.last_known_value()
    }

    #[must_use]
    pub fn sources(&self) -> &[SourceStatus] {
        self.value.sources()
    }
}
