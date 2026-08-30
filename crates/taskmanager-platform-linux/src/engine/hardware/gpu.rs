//! Linux GPU telemetry: DRM-card scanning plus per-vendor provider submodules.
//!
//! Builds per-card `GpuMetrics` from `/sys/class/drm`, derives Intel `i915`/`xe`
//! usage from RC6 idle-residency deltas, and merges vendor samples (amdgpu,
//! intel, nvidia) through the GPU provider registry.

use std::fs;
use std::path::{Path, PathBuf};
#[cfg(any(test, feature = "test-support"))]
use std::{collections::HashMap, time::Instant};
use taskmanager_core::FailureKind;
use taskmanager_core::core::device_state::{DeviceState, DeviceStatus, stable_gpu_id};
use taskmanager_core::core::metrics::GpuThrottleReason;
use taskmanager_core::core::metrics::{
    GpuEngine, GpuEngineKind, GpuMetricField, GpuMetrics, GpuScalarObservations, ScalarObservation,
};

use super::read_sysfs_string;

mod amd;
mod api;
mod field_read;
mod intel;
mod nvidia;
mod parsing;
mod provider;
use super::pci_ids::{parse_pci_id, read_pci_marketing_name};
use amd::probe_amdgpu_device;
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", allow(unused_imports))]
use amd::read_amdgpu_engines;
use field_read::{GpuFieldRead, gpu_io_failure, preferred_gpu_failure};
#[cfg(any(test, feature = "test-support"))]
use intel::read_intel_gt_rc6_residency_ms;
use intel::{
    EngineBusySource, IntelEngineRead, read_intel_gt_engines, read_intel_gt_frequency,
    read_intel_gt_max_frequency, read_intel_gt_rc6_residency,
};
#[cfg(any(test, feature = "test-support"))]
use intel::{read_intel_gt_freq_mhz, read_intel_gt_max_freq_mhz};
#[cfg(any(test, feature = "test-support"))]
use nvidia::append_nvidia_procfs;
#[cfg(feature = "nvidia")]
use nvidia::collect_nvidia_nvml;
#[cfg(any(test, feature = "nvidia"))]
pub(crate) use parsing::normalize_pci_slot;
use parsing::{
    compose_intel_brand, parse_busy_percent, parse_driver_name, parse_module_version,
    parse_nvrm_driver_version,
};
pub(crate) use provider::GpuProviderRegistry;
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", allow(unused_imports))]
use provider::merge_provider_samples;

#[derive(Debug, Clone)]
struct GpuProviderSample {
    metrics: GpuMetrics,
    fields: Vec<GpuMetricField>,
    /// Independently failed fields from a partially successful runtime read.
    ///
    /// This stays Linux-internal: vendor APIs map native errors into the
    /// shared failure vocabulary before deterministic provider merge.
    field_failures: Vec<GpuProviderFieldFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GpuProviderFieldFailure {
    field: GpuMetricField,
    failure: FailureKind,
}

/// Read the kernel driver bound to a DRM PCI device — the basename of the
/// `device/driver` symlink (e.g. `xe`, `i915`, `amdgpu`, `nvidia`). Returns
/// `None` when the symlink is absent (no bound driver) or unreadable. The
/// driver disambiguates Intel `xe` from `i915` and identifies the part for the
/// UI's "Driver" stat.
fn read_driver_name(device_path: &Path) -> Option<String> {
    let drv_link = device_path.join("driver");
    fs::read_link(&drv_link)
        .ok()
        .and_then(|p| parse_driver_name(&p.to_string_lossy()))
}

/// Read the release a kernel module declares about itself through
/// `<module_root>/<driver>/version`. The node only exists for modules that set
/// a module version: the out-of-tree `nvidia` module carries one, while the
/// in-tree DRM drivers (`xe`, `i915`, `amdgpu`, `nouveau`, `radeon`) ship with
/// the kernel and expose no independent version — for those this returns
/// `None` (an honest absence, never the kernel release misfiled as a driver
/// version).
fn read_kernel_module_version(module_root: &Path, driver: &str) -> Option<String> {
    fs::read_to_string(module_root.join(driver).join("version"))
        .ok()
        .and_then(|raw| parse_module_version(&raw))
}

/// Scans DRM devices (under `drm_base`) plus NVIDIA procfs (`nvidia_base`) to
/// detect GPU metrics without consulting the live host. STATELESS (no per-tick
/// rate): on Intel i915/xe
/// utilization stays Unknown here because a real usage figure needs the RC6
/// residency delta over time. The live collector uses
/// [`detect_gpu_metrics_with_rc6`] for that. This pure entry point is used by
/// unit tests (which pass synthetic paths) and by
/// `detect_gpu_metrics_with_rc6_from_paths`. NVML is deliberately applied only
/// by the live entry point so fixture results never depend on host hardware.
#[cfg(any(test, feature = "test-support"))]
pub fn detect_gpu_metrics_from_paths(
    drm_base: &Path,
    nvidia_base: &Path,
    module_base: &Path,
) -> Vec<GpuMetrics> {
    let mut gpus = Vec::new();
    for (card_name, device_path) in scan_drm_cards(drm_base) {
        gpus.push(build_drm_card_metrics(
            &card_name,
            &device_path,
            module_base,
        ));
    }
    append_nvidia_procfs(nvidia_base, &mut gpus);
    gpus
}

#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", allow(dead_code))]
pub fn detect_gpu_metrics_with_rc6_from_paths(
    drm_base: &Path,
    nvidia_base: &Path,
    module_base: &Path,
    prev_rc6: &mut HashMap<String, (u64, Instant)>,
    now: Instant,
) -> Vec<GpuMetrics> {
    let mut gpus = Vec::new();
    for (card_name, device_path) in scan_drm_cards(drm_base) {
        let mut m = build_drm_card_metrics(&card_name, &device_path, module_base);
        // Intel i915/xe: derive real usage from RC6 idle residency. amdgpu's
        // `gpu_busy_percent` (read above) and NVIDIA NVML are left as-is. The
        // guard matches every composed Intel brand ("Intel", "Intel Graphics",
        // "Intel Xe Graphics") via `starts_with`; the residency read itself is
        // Intel-specific (tile*/gt*/gtidle) and returns None elsewhere anyway.
        if m.brand.starts_with("Intel")
            && let Some(curr) = read_intel_gt_rc6_residency_ms(&device_path)
        {
            if let Some((prev_res, prev_t)) = prev_rc6.get(&m.device_id) {
                let dt = now.duration_since(*prev_t).as_secs_f32();
                if dt > 0.0 {
                    // Monotonic counter; saturating_sub guards against the
                    // rare driver reset that makes it jump backwards.
                    let delta = curr.saturating_sub(*prev_res);
                    let rc6_pct = ((delta as f32 / (dt * 1000.0)) * 100.0).clamp(0.0, 100.0);
                    let mut observations = *m.scalar_observations();
                    observations.utilization_pct = ScalarObservation::available(100.0 - rc6_pct, 0);
                    observations.idle_residency_pct = ScalarObservation::available(rc6_pct, 0);
                    m.apply_scalar_observations(observations);
                }
            }
            prev_rc6.insert(m.device_id.clone(), (curr, now));
        }
        gpus.push(m);
    }
    append_nvidia_procfs(nvidia_base, &mut gpus);
    gpus
}

/// Sorted `(card_name, device_path)` pairs for the `cardN` entries (excluding
/// the `cardN-connector` outputs) under a DRM root such as `/sys/class/drm`.
/// `device_path` is `<card>/device` (a symlink to the PCI device, e.g.
/// `/sys/devices/pci0000:00/0000:00:02.0`); all per-card reads join onto it and
/// transparently follow the symlink.
fn scan_drm_cards(drm_base: &Path) -> Vec<(String, PathBuf)> {
    let mut cards = Vec::new();
    if !drm_base.exists() {
        return cards;
    }
    let Ok(entries) = fs::read_dir(drm_base) else {
        return cards;
    };
    for e in entries.flatten() {
        let name = e.file_name();
        let s = name.to_string_lossy();
        if s.starts_with("card") && !s.contains('-') {
            cards.push((s.to_string(), e.path().join("device")));
        }
    }
    cards.sort_by(|a, b| a.0.cmp(&b.0));
    cards
}

/// Compose the generic DRM identity with runtime-selected enrichments for the
/// path-injected parser API. The live collector uses separate providers.
#[cfg(any(test, feature = "test-support"))]
fn build_drm_card_metrics(card_name: &str, device_path: &Path, module_root: &Path) -> GpuMetrics {
    let mut metric = build_drm_identity_metrics(card_name, device_path, module_root);
    let mut observations = *metric.scalar_observations();
    let mut throttle_observation = metric.throttle_observation().clone();
    if let Some(sample) = probe_amdgpu_device(card_name, device_path).sample {
        for field in sample.fields {
            apply_gpu_metric_field(
                &mut metric,
                &sample.metrics,
                field,
                &mut observations,
                &mut throttle_observation,
            );
        }
    }
    if observations.frequency_mhz.current_value().is_none()
        && let Some(value) = read_intel_gt_freq_mhz(device_path)
    {
        observations.frequency_mhz = ScalarObservation::available(value, 0);
    }
    if observations.max_frequency_mhz.current_value().is_none()
        && let Some(value) = read_intel_gt_max_freq_mhz(device_path)
    {
        observations.max_frequency_mhz = ScalarObservation::available(value, 0);
    }
    metric.apply_scalar_observations(observations);
    metric.apply_throttle_observation(throttle_observation);
    metric
}

/// Generic DRM inventory fact: stable PCI/card identity, vendor label, bound
/// kernel driver and — only when the bound module itself declares one — the
/// kernel driver version. It deliberately performs no vendor telemetry reads.
fn build_drm_identity_metrics(
    _card_name: &str,
    device_path: &Path,
    module_root: &Path,
) -> GpuMetrics {
    let driver = read_driver_name(device_path);
    let driver_version = driver
        .as_deref()
        .and_then(|name| read_kernel_module_version(module_root, name));
    let pci_slot = read_pci_slot_name(device_path);
    let pci_vendor_id = read_pci_id(device_path, "vendor");
    let pci_device_id = read_pci_id(device_path, "device");
    let pci_subsystem_vendor_id = read_pci_id(device_path, "subsystem_vendor");
    let pci_subsystem_device_id = read_pci_id(device_path, "subsystem_device");
    let pci_modalias = read_sysfs_string(&device_path.join("modalias").to_string_lossy());
    let marketing_name = pci_vendor_id
        .zip(pci_device_id)
        .and_then(|(vendor, device)| read_pci_marketing_name(vendor, device));
    let mut brand = "Generic GPU".to_string();
    if let Some(vendor_str) = read_sysfs_string(&device_path.join("vendor").to_string_lossy()) {
        let v_lower = vendor_str.to_lowercase();
        if v_lower.contains("0x1002") {
            brand = "AMD".to_string();
        } else if v_lower.contains("0x8086") {
            brand = compose_intel_brand(driver.as_deref()).to_string();
        } else if v_lower.contains("0x10de") {
            brand = "NVIDIA".to_string();
        } else {
            brand = vendor_str;
        }
    }

    let mut metrics = GpuMetrics::new(linux_gpu_device_id(device_path, pci_slot.as_deref()), brand);
    metrics.device_state = DeviceState {
        status: DeviceStatus::Healthy,
        last_success_ms: None,
    };
    metrics.marketing_name = marketing_name;
    metrics.pci_vendor_id = pci_vendor_id;
    metrics.pci_device_id = pci_device_id;
    metrics.pci_subsystem_vendor_id = pci_subsystem_vendor_id;
    metrics.pci_subsystem_device_id = pci_subsystem_device_id;
    metrics.pci_slot = pci_slot;
    metrics.pci_modalias = pci_modalias;
    metrics.driver = driver;
    metrics.driver_version = driver_version;
    metrics
}

fn linux_gpu_device_id(device_path: &Path, pci_slot: Option<&str>) -> String {
    if pci_slot.is_some() {
        return stable_gpu_id("drm", pci_slot);
    }
    // A DRM card number is enumeration order, not identity. Non-PCI devices
    // keep the canonical native attachment path; if even that cannot be
    // proven, return no identity so the merge layer drops the row.
    std::fs::canonicalize(device_path).map_or_else(
        |_| String::new(),
        |path| format!("gpu:sysfs:{}", path.to_string_lossy()),
    )
}

fn apply_gpu_metric_field(
    target: &mut GpuMetrics,
    sample: &GpuMetrics,
    field: GpuMetricField,
    observations: &mut GpuScalarObservations,
    throttle_observation: &mut ScalarObservation<Vec<GpuThrottleReason>>,
) {
    let sample_observations = sample.scalar_observations();
    match field {
        GpuMetricField::Identity => {
            target.device_id.clone_from(&sample.device_id);
            if sample.marketing_name.is_some() {
                target.marketing_name.clone_from(&sample.marketing_name);
            }
            if sample.pci_vendor_id.is_some() {
                target.pci_vendor_id = sample.pci_vendor_id;
            }
            if sample.pci_device_id.is_some() {
                target.pci_device_id = sample.pci_device_id;
            }
            if sample.pci_subsystem_vendor_id.is_some() {
                target.pci_subsystem_vendor_id = sample.pci_subsystem_vendor_id;
            }
            if sample.pci_subsystem_device_id.is_some() {
                target.pci_subsystem_device_id = sample.pci_subsystem_device_id;
            }
            if sample.pci_slot.is_some() {
                target.pci_slot.clone_from(&sample.pci_slot);
            }
            if sample.pci_modalias.is_some() {
                target.pci_modalias.clone_from(&sample.pci_modalias);
            }
        }
        GpuMetricField::Brand => target.brand.clone_from(&sample.brand),
        GpuMetricField::GraphicsApi => target.graphics_api.clone_from(&sample.graphics_api),
        GpuMetricField::Utilization => {
            observations.utilization_pct = sample_observations.utilization_pct;
        }
        GpuMetricField::IdleResidency => {
            observations.idle_residency_pct = sample_observations.idle_residency_pct;
        }
        GpuMetricField::Memory => {
            observations.memory_used_bytes = sample_observations.memory_used_bytes;
            observations.memory_total_bytes = sample_observations.memory_total_bytes;
            observations.dedicated_vram_used_bytes = sample_observations.dedicated_vram_used_bytes;
            observations.dedicated_vram_total_bytes =
                sample_observations.dedicated_vram_total_bytes;
            observations.shared_vram_used_bytes = sample_observations.shared_vram_used_bytes;
            observations.shared_vram_total_bytes = sample_observations.shared_vram_total_bytes;
        }
        GpuMetricField::DedicatedVram => {
            observations.dedicated_vram_used_bytes = sample_observations.dedicated_vram_used_bytes;
            observations.dedicated_vram_total_bytes =
                sample_observations.dedicated_vram_total_bytes;
        }
        GpuMetricField::SharedVram => {
            observations.shared_vram_used_bytes = sample_observations.shared_vram_used_bytes;
            observations.shared_vram_total_bytes = sample_observations.shared_vram_total_bytes;
        }
        GpuMetricField::Engines => target.engines.clone_from(&sample.engines),
        GpuMetricField::Temperature => {
            observations.temperature_c = sample_observations.temperature_c;
        }
        GpuMetricField::Power => observations.power_w = sample_observations.power_w,
        GpuMetricField::Fan => {
            observations.fan_speed_rpm = sample_observations.fan_speed_rpm;
            observations.fan_speed_pct = sample_observations.fan_speed_pct;
        }
        GpuMetricField::Frequency => {
            observations.frequency_mhz = sample_observations.frequency_mhz;
            observations.max_frequency_mhz = sample_observations.max_frequency_mhz;
        }
        GpuMetricField::Throttle => throttle_observation.clone_from(sample.throttle_observation()),
        GpuMetricField::Driver => target.driver.clone_from(&sample.driver),
        GpuMetricField::DriverVersion => {
            target.driver_version.clone_from(&sample.driver_version);
        }
    }
}

fn read_pci_slot_name(device_path: &Path) -> Option<String> {
    let uevent = fs::read_to_string(device_path.join("uevent")).ok()?;
    uevent.lines().find_map(|line| {
        line.strip_prefix("PCI_SLOT_NAME=")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn read_pci_id(device_path: &Path, file: &str) -> Option<u16> {
    let raw = read_sysfs_string(&device_path.join(file).to_string_lossy())?;
    parse_pci_id(&raw)
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_hardware_gpu_tests.rs"]
mod tests;
