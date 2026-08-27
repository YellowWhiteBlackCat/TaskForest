//! Private compatibility shape for historical GPU snapshot JSON.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::*;
use crate::core::device_state::DeviceStatus;

/// Legacy live keys are optional here so a non-current typed observation is
/// omitted instead of being serialized as a believable zero or empty value.
#[derive(Serialize, Deserialize, Default)]
struct GpuMetricsWire {
    #[serde(default)]
    device_id: String,
    #[serde(default)]
    device_generation: Option<DeviceGeneration>,
    #[serde(default)]
    device_state: Option<DeviceState>,
    #[serde(default)]
    provenance: Option<Vec<GpuMetricProvenance>>,
    #[serde(default)]
    scalar_observations: GpuScalarObservations,
    #[serde(default)]
    throttle_observation: ScalarObservation<Vec<GpuThrottleReason>>,
    #[serde(default)]
    brand: String,
    #[serde(default)]
    marketing_name: Option<String>,
    #[serde(default)]
    pci_vendor_id: Option<u16>,
    #[serde(default)]
    pci_device_id: Option<u16>,
    #[serde(default)]
    pci_subsystem_vendor_id: Option<u16>,
    #[serde(default)]
    pci_subsystem_device_id: Option<u16>,
    #[serde(default)]
    pci_slot: Option<String>,
    #[serde(default)]
    pci_modalias: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    gpu_usage_pct: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    utilization_pct: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    vram_used_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    vram_total_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dedicated_vram_used_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dedicated_vram_total_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    shared_vram_used_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    shared_vram_total_bytes: Option<u64>,
    #[serde(default)]
    engines: Vec<GpuEngine>,
    #[serde(default)]
    engine_failure: Option<FailureKind>,
    #[serde(default)]
    engine_provider: Option<ProviderId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    temp_celsius: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    temperature_c: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    gpu_power_w: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fan_speed_rpm: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fan_speed_pct: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rc6_idle_pct: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    idle_residency_pct: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    memory_used_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    memory_total_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    gpu_freq_mhz: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_freq_mhz: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    gpu_throttle_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    throttle_reasons: Option<Vec<GpuThrottleReason>>,
    #[serde(default)]
    driver: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    graphics_api: Option<GpuGraphicsApi>,
}

impl Serialize for GpuMetrics {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let utilization = legacy_scalar_projection(&self.scalar_observations.utilization_pct);
        let dedicated_used =
            legacy_scalar_projection(&self.scalar_observations.dedicated_vram_used_bytes);
        let dedicated_total =
            legacy_scalar_projection(&self.scalar_observations.dedicated_vram_total_bytes);
        let throttle_reasons = legacy_scalar_projection(&self.throttle_observation);
        let gpu_throttle_reason = throttle_reasons
            .as_ref()
            .map(|reasons| throttle_reason_text(reasons));
        GpuMetricsWire {
            device_id: self.device_id.clone(),
            device_generation: Some(self.device_generation),
            device_state: Some(self.device_state),
            provenance: Some(self.provenance.clone()),
            scalar_observations: self.scalar_observations,
            throttle_observation: self.throttle_observation.clone(),
            brand: self.brand.clone(),
            marketing_name: self.marketing_name.clone(),
            pci_vendor_id: self.pci_vendor_id,
            pci_device_id: self.pci_device_id,
            pci_subsystem_vendor_id: self.pci_subsystem_vendor_id,
            pci_subsystem_device_id: self.pci_subsystem_device_id,
            pci_slot: self.pci_slot.clone(),
            pci_modalias: self.pci_modalias.clone(),
            gpu_usage_pct: utilization,
            utilization_pct: utilization,
            vram_used_bytes: dedicated_used,
            vram_total_bytes: dedicated_total,
            dedicated_vram_used_bytes: dedicated_used,
            dedicated_vram_total_bytes: dedicated_total,
            shared_vram_used_bytes: legacy_scalar_projection(
                &self.scalar_observations.shared_vram_used_bytes,
            ),
            shared_vram_total_bytes: legacy_scalar_projection(
                &self.scalar_observations.shared_vram_total_bytes,
            ),
            engines: self.engines.clone(),
            engine_failure: self.engine_failure,
            engine_provider: self.engine_provider.clone(),
            temp_celsius: legacy_scalar_projection(&self.scalar_observations.temperature_c),
            temperature_c: legacy_scalar_projection(&self.scalar_observations.temperature_c),
            gpu_power_w: legacy_scalar_projection(&self.scalar_observations.power_w),
            fan_speed_rpm: legacy_scalar_projection(&self.scalar_observations.fan_speed_rpm),
            fan_speed_pct: legacy_scalar_projection(&self.scalar_observations.fan_speed_pct),
            rc6_idle_pct: legacy_scalar_projection(&self.scalar_observations.idle_residency_pct),
            idle_residency_pct: legacy_scalar_projection(
                &self.scalar_observations.idle_residency_pct,
            ),
            memory_used_bytes: legacy_scalar_projection(
                &self.scalar_observations.memory_used_bytes,
            ),
            memory_total_bytes: legacy_scalar_projection(
                &self.scalar_observations.memory_total_bytes,
            ),
            gpu_freq_mhz: legacy_scalar_projection(&self.scalar_observations.frequency_mhz),
            max_freq_mhz: legacy_scalar_projection(&self.scalar_observations.max_frequency_mhz),
            gpu_throttle_reason,
            throttle_reasons,
            driver: self.driver.clone(),
            graphics_api: self.graphics_api.clone(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for GpuMetrics {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = GpuMetricsWire::deserialize(deserializer)?;
        // The immediately preceding derived-Serde envelope always emitted
        // generation zero, an Unsupported/no-success state and empty
        // provenance. Preserve that schema-v1 envelope while rejecting an
        // explicit non-default failure state.
        let trusted_base = !wire.device_id.trim().is_empty()
            && wire.device_state.is_none_or(|state| {
                state.status == DeviceStatus::Healthy
                    || (state.status == DeviceStatus::Unsupported
                        && state.last_success_ms.is_none())
            });
        let trusted_field = |field| {
            trusted_base
                && wire.provenance.as_ref().is_none_or(|items| {
                    items.is_empty() || items.iter().any(|item| item.field == field)
                })
        };
        let observed_at_ms = wire
            .device_state
            .and_then(|state| state.last_success_ms)
            .unwrap_or(0);
        let mut observations = wire.scalar_observations;
        observations.utilization_pct = hydrate_legacy_scalar(
            observations.utilization_pct,
            trusted_field(GpuMetricField::Utilization)
                .then_some(
                    wire.utilization_pct
                        .or_else(|| wire.gpu_usage_pct.filter(|value| *value != 0.0)),
                )
                .flatten(),
            observed_at_ms,
        );
        observations.temperature_c = hydrate_legacy_scalar(
            observations.temperature_c,
            trusted_field(GpuMetricField::Temperature)
                .then_some(
                    wire.temperature_c
                        .or_else(|| wire.temp_celsius.filter(|value| *value != 0.0)),
                )
                .flatten(),
            observed_at_ms,
        );
        let aggregate_total = wire
            .memory_total_bytes
            .filter(|value| *value > 0)
            .or_else(|| wire.vram_total_bytes.filter(|value| *value > 0));
        observations.memory_used_bytes = hydrate_legacy_scalar(
            observations.memory_used_bytes,
            trusted_field(GpuMetricField::Memory)
                .then_some(aggregate_total.and_then(|_| {
                    wire.memory_used_bytes
                        .or_else(|| wire.vram_used_bytes.filter(|value| *value > 0))
                }))
                .flatten(),
            observed_at_ms,
        );
        observations.memory_total_bytes = hydrate_legacy_scalar(
            observations.memory_total_bytes,
            trusted_field(GpuMetricField::Memory)
                .then_some(aggregate_total)
                .flatten(),
            observed_at_ms,
        );
        let trusted_dedicated =
            trusted_field(GpuMetricField::DedicatedVram) || trusted_field(GpuMetricField::Memory);
        observations.dedicated_vram_used_bytes = hydrate_legacy_scalar(
            observations.dedicated_vram_used_bytes,
            trusted_dedicated
                .then_some(
                    wire.dedicated_vram_used_bytes
                        .filter(|value| *value > 0)
                        .or_else(|| wire.vram_used_bytes.filter(|value| *value > 0)),
                )
                .flatten(),
            observed_at_ms,
        );
        observations.dedicated_vram_total_bytes = hydrate_legacy_scalar(
            observations.dedicated_vram_total_bytes,
            trusted_dedicated
                .then_some(
                    wire.dedicated_vram_total_bytes
                        .filter(|value| *value > 0)
                        .or_else(|| wire.vram_total_bytes.filter(|value| *value > 0)),
                )
                .flatten(),
            observed_at_ms,
        );
        observations.shared_vram_used_bytes = hydrate_legacy_scalar(
            observations.shared_vram_used_bytes,
            trusted_field(GpuMetricField::SharedVram)
                .then_some(wire.shared_vram_used_bytes.filter(|value| *value > 0))
                .flatten(),
            observed_at_ms,
        );
        observations.shared_vram_total_bytes = hydrate_legacy_scalar(
            observations.shared_vram_total_bytes,
            trusted_field(GpuMetricField::SharedVram)
                .then_some(wire.shared_vram_total_bytes.filter(|value| *value > 0))
                .flatten(),
            observed_at_ms,
        );
        observations.frequency_mhz = hydrate_legacy_scalar(
            observations.frequency_mhz,
            trusted_field(GpuMetricField::Frequency)
                .then_some(wire.gpu_freq_mhz.filter(|value| *value > 0))
                .flatten(),
            observed_at_ms,
        );
        observations.max_frequency_mhz = hydrate_legacy_scalar(
            observations.max_frequency_mhz,
            trusted_field(GpuMetricField::Frequency)
                .then_some(wire.max_freq_mhz.filter(|value| *value > 0))
                .flatten(),
            observed_at_ms,
        );
        observations.fan_speed_rpm = hydrate_legacy_scalar(
            observations.fan_speed_rpm,
            trusted_field(GpuMetricField::Fan)
                .then_some(wire.fan_speed_rpm)
                .flatten(),
            observed_at_ms,
        );
        observations.fan_speed_pct = hydrate_legacy_scalar(
            observations.fan_speed_pct,
            trusted_field(GpuMetricField::Fan)
                .then_some(wire.fan_speed_pct)
                .flatten(),
            observed_at_ms,
        );
        observations.power_w = hydrate_legacy_scalar(
            observations.power_w,
            trusted_field(GpuMetricField::Power)
                .then_some(wire.gpu_power_w)
                .flatten(),
            observed_at_ms,
        );
        observations.idle_residency_pct = hydrate_legacy_scalar(
            observations.idle_residency_pct,
            trusted_field(GpuMetricField::IdleResidency)
                .then_some(wire.idle_residency_pct.or(wire.rc6_idle_pct))
                .flatten(),
            observed_at_ms,
        );
        let explicit_current_throttle = wire.device_state.is_some_and(|state| {
            state.status == DeviceStatus::Healthy && state.last_success_ms.is_some()
        }) && wire.provenance.as_ref().is_some_and(|items| {
            items
                .iter()
                .any(|item| item.field == GpuMetricField::Throttle)
        });
        let legacy_throttle = wire
            .throttle_reasons
            .filter(|reasons| !reasons.is_empty() || explicit_current_throttle)
            .or_else(|| {
                wire.gpu_throttle_reason.as_deref().and_then(|text| {
                    (!text.trim().is_empty() || explicit_current_throttle)
                        .then(|| parse_throttle_text(text))
                })
            });
        let throttle_observation = hydrate_legacy_scalar(
            wire.throttle_observation,
            trusted_field(GpuMetricField::Throttle)
                .then_some(legacy_throttle)
                .flatten(),
            observed_at_ms,
        );
        Ok(Self {
            device_id: wire.device_id,
            device_generation: wire.device_generation.unwrap_or_default(),
            device_state: wire.device_state.unwrap_or_default(),
            provenance: wire.provenance.unwrap_or_default(),
            scalar_observations: observations,
            throttle_observation,
            brand: wire.brand,
            marketing_name: wire.marketing_name,
            pci_vendor_id: wire.pci_vendor_id,
            pci_device_id: wire.pci_device_id,
            pci_subsystem_vendor_id: wire.pci_subsystem_vendor_id,
            pci_subsystem_device_id: wire.pci_subsystem_device_id,
            pci_slot: wire.pci_slot,
            pci_modalias: wire.pci_modalias,
            engines: wire.engines,
            engine_failure: wire.engine_failure,
            engine_provider: wire.engine_provider,
            driver: wire.driver,
            graphics_api: wire.graphics_api,
        })
    }
}
