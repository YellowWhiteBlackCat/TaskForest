//! Read capability for runtime power-supply and sensor histories.

use std::sync::Arc;

use taskmanager_core::DeviceId;

use super::{
    CorrelatedSystemTelemetryHistoryInner, DeviceMetricHistory, DynamicHistoryDomain,
    device_history,
};

/// Read-only history for runtime power supplies and sensor channels.
#[derive(Clone)]
pub struct DynamicTelemetryHistory {
    inner: Arc<CorrelatedSystemTelemetryHistoryInner>,
}

impl DynamicTelemetryHistory {
    pub(super) fn new(inner: Arc<CorrelatedSystemTelemetryHistoryInner>) -> Self {
        Self { inner }
    }

    #[must_use]
    pub fn battery_capacity_pct(&self, device_id: &DeviceId) -> Option<DeviceMetricHistory<f32>> {
        device_history(
            &self.inner.battery_capacity_pct,
            device_id,
            &self.inner.dynamic_commit_gates[DynamicHistoryDomain::Power.index()],
        )
    }

    #[must_use]
    pub fn battery_power_w(&self, device_id: &DeviceId) -> Option<DeviceMetricHistory<f32>> {
        device_history(
            &self.inner.battery_power_w,
            device_id,
            &self.inner.dynamic_commit_gates[DynamicHistoryDomain::Power.index()],
        )
    }

    #[must_use]
    pub fn fan_rpm(&self, device_id: &DeviceId) -> Option<DeviceMetricHistory<f32>> {
        device_history(
            &self.inner.fan_rpm,
            device_id,
            &self.inner.dynamic_commit_gates[DynamicHistoryDomain::Sensor.index()],
        )
    }

    #[must_use]
    pub fn fan_pwm_pct(&self, device_id: &DeviceId) -> Option<DeviceMetricHistory<f32>> {
        device_history(
            &self.inner.fan_pwm_pct,
            device_id,
            &self.inner.dynamic_commit_gates[DynamicHistoryDomain::Sensor.index()],
        )
    }

    #[must_use]
    pub fn fan_temperature_c(&self, device_id: &DeviceId) -> Option<DeviceMetricHistory<f32>> {
        device_history(
            &self.inner.fan_temperature_c,
            device_id,
            &self.inner.dynamic_commit_gates[DynamicHistoryDomain::Sensor.index()],
        )
    }
}
