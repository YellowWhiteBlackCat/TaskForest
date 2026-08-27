//! Storage system telemetry observation: wraps `Vec<DiskMetrics>` in a
//! freshness-typed domain value with storage discovery lifecycle
//! (`DeviceLifecycle`) and per-provider runtime state.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::domain::{SystemDomainValue, SystemObservationState};
use super::{ProviderRuntimeState, sorted_provider_states};
use crate::core::{DeviceId, DeviceLifecycle, DiskMetrics, FailureKind, SourceStatus};

/// Storage observations own storage discovery lifecycle and provider health.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorageTelemetryObservation {
    value: SystemDomainValue<Vec<DiskMetrics>>,
    provider_states: Vec<ProviderRuntimeState>,
    device_lifecycles: BTreeMap<DeviceId, DeviceLifecycle>,
}

impl StorageTelemetryObservation {
    #[must_use]
    pub fn current(
        value: Vec<DiskMetrics>,
        observed_at_ms: u64,
        sources: Vec<SourceStatus>,
        provider_states: Vec<ProviderRuntimeState>,
        device_lifecycles: BTreeMap<DeviceId, DeviceLifecycle>,
    ) -> Self {
        Self {
            value: SystemDomainValue::current(value, observed_at_ms, sources),
            provider_states: sorted_provider_states(provider_states),
            device_lifecycles,
        }
    }

    #[must_use]
    pub fn partial(
        value: Vec<DiskMetrics>,
        observed_at_ms: u64,
        failure: FailureKind,
        sources: Vec<SourceStatus>,
        provider_states: Vec<ProviderRuntimeState>,
        device_lifecycles: BTreeMap<DeviceId, DeviceLifecycle>,
    ) -> Self {
        Self {
            value: SystemDomainValue::partial(value, observed_at_ms, failure, sources),
            provider_states: sorted_provider_states(provider_states),
            device_lifecycles,
        }
    }

    #[must_use]
    pub fn stale(
        last_value: Vec<DiskMetrics>,
        last_success_ms: u64,
        failure: FailureKind,
        sources: Vec<SourceStatus>,
        provider_states: Vec<ProviderRuntimeState>,
        device_lifecycles: BTreeMap<DeviceId, DeviceLifecycle>,
    ) -> Self {
        Self {
            value: SystemDomainValue::stale(last_value, last_success_ms, failure, sources),
            provider_states: sorted_provider_states(provider_states),
            device_lifecycles,
        }
    }

    #[must_use]
    pub fn unavailable(
        failure: FailureKind,
        sources: Vec<SourceStatus>,
        provider_states: Vec<ProviderRuntimeState>,
        device_lifecycles: BTreeMap<DeviceId, DeviceLifecycle>,
    ) -> Self {
        Self {
            value: SystemDomainValue::unavailable(failure, sources),
            provider_states: sorted_provider_states(provider_states),
            device_lifecycles,
        }
    }

    #[must_use]
    pub const fn state(&self) -> SystemObservationState {
        self.value.state()
    }

    #[must_use]
    pub fn current_value(&self) -> Option<&[DiskMetrics]> {
        self.value.current_value().map(Vec::as_slice)
    }

    #[must_use]
    pub fn last_known_value(&self) -> Option<&[DiskMetrics]> {
        self.value.last_known_value().map(Vec::as_slice)
    }

    #[must_use]
    pub fn sources(&self) -> &[SourceStatus] {
        self.value.sources()
    }

    #[must_use]
    pub fn provider_states(&self) -> &[ProviderRuntimeState] {
        &self.provider_states
    }

    #[must_use]
    pub const fn device_lifecycles(&self) -> &BTreeMap<DeviceId, DeviceLifecycle> {
        &self.device_lifecycles
    }
}

#[cfg(test)]
#[path = "../../../../tests/headless/core_core_metrics_system_storage_storage_observation_tests.rs"]
mod storage_observation_tests;
