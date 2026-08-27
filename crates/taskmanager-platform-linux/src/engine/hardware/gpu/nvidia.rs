//! NVIDIA runtime enrichment for the generic DRM/procfs GPU baseline.
//! `LINUX-NVIDIA-02` covers field-isolated fan and typed throttle reads.

use super::*;

#[cfg(feature = "nvidia")]
mod throttle;

#[cfg(feature = "nvidia")]
use crate::engine::nvml::{NvmlFailureKind, classify_error};
#[cfg(feature = "nvidia")]
use throttle::read_throttle_reasons;

/// Append NVIDIA boards discovered through the kernel driver's procfs tree.
///
/// This fallback is always compiled. NVML enriches it at runtime when the
/// userspace library is available, but lack of NVML must never hide a board
/// that the kernel already exposes.
#[cfg(any(test, feature = "test-support"))]
pub(super) fn append_nvidia_procfs(nvidia_base: &Path, gpus: &mut Vec<GpuMetrics>) {
    let Ok(samples) = collect_nvidia_procfs_samples(nvidia_base) else {
        return;
    };
    for sample in samples {
        let metric = sample.metrics;
        if !gpus.iter().any(|gpu| gpu.device_id == metric.device_id) {
            gpus.push(metric);
        }
    }
}

pub(super) fn collect_nvidia_procfs_samples(
    nvidia_base: &Path,
) -> Result<Vec<GpuProviderSample>, DeviceStatus> {
    let gpus_dir = nvidia_base.join("gpus");
    let gpu_entries = fs::read_dir(gpus_dir).map_err(|error| match error.kind() {
        std::io::ErrorKind::PermissionDenied => DeviceStatus::PermissionDenied,
        std::io::ErrorKind::NotFound => DeviceStatus::Unsupported,
        _ => DeviceStatus::Stale,
    })?;
    let mut samples = Vec::new();
    for gpu_entry in gpu_entries {
        let gpu_entry = gpu_entry.map_err(|error| match error.kind() {
            std::io::ErrorKind::PermissionDenied => DeviceStatus::PermissionDenied,
            _ => DeviceStatus::Stale,
        })?;
        let information = fs::read_to_string(gpu_entry.path().join("information")).map_err(
            |error| match error.kind() {
                std::io::ErrorKind::PermissionDenied => DeviceStatus::PermissionDenied,
                _ => DeviceStatus::Stale,
            },
        )?;
        let brand = information
            .lines()
            .find_map(|line| {
                line.split_once(':').and_then(|(key, value)| {
                    key.trim()
                        .eq_ignore_ascii_case("model")
                        .then(|| value.trim())
                        .filter(|value| !value.is_empty())
                        .map(ToOwned::to_owned)
                })
            })
            .unwrap_or_else(|| "NVIDIA".to_string());
        let slot = gpu_entry.file_name().to_string_lossy().into_owned();
        let mut metrics = GpuMetrics::new(stable_gpu_id("nvidia", Some(&slot)), brand);
        metrics.device_state = DeviceState {
            status: DeviceStatus::Healthy,
            last_success_ms: None,
        };
        samples.push(GpuProviderSample {
            metrics,
            fields: vec![GpuMetricField::Identity, GpuMetricField::Brand],
            field_failures: Vec::new(),
        });
    }
    Ok(samples)
}

/// Collect NVIDIA GPU metrics from the dynamically loaded NVML runtime. The
/// backend is part of every standard artifact; absence of
/// `libnvidia-ml`, a device, or an individual metric is a runtime capability
/// result rather than a reason to ship a vendor-specific binary.
///
/// Every NVML call is individually fallible. PCI identity merges a rich NVML
/// sample into the DRM/procfs device instead of dropping it merely because both
/// sources reported the same board or model. This host has no NVIDIA device, so
/// final live acceptance still requires the target-host receipt.
#[cfg(feature = "nvidia")]
pub(super) fn collect_nvidia_nvml() -> Result<Vec<GpuProviderSample>, DeviceStatus> {
    use nvml_wrapper::Nvml;

    let nvml = Nvml::init().map_err(|error| classify_error(&error).device_status())?;
    let count = nvml
        .device_count()
        .map_err(|error| classify_error(&error).device_status())?;
    if count == 0 {
        return Err(NvmlFailureKind::Unsupported.device_status());
    }

    let mut samples = Vec::new();
    let mut failures = Vec::new();
    for index in 0..count {
        let device = match nvml.device_by_index(index) {
            Ok(device) => device,
            Err(error) => {
                failures.push(classify_error(&error));
                continue;
            }
        };
        let assembly = assemble_nvml_device(read_nvml_device(&device));
        for failure in assembly.failures {
            tracing::trace!(
                field = ?failure.field,
                failure = ?failure.kind,
                "NVML field unavailable"
            );
            failures.push(failure.kind);
        }
        if let Some(sample) = assembly.sample {
            samples.push(sample);
        }
    }

    if samples.is_empty() {
        Err(preferred_failure(&failures).device_status())
    } else {
        Ok(samples)
    }
}

#[cfg(feature = "nvidia")]
#[derive(Debug)]
struct NvmlDeviceReadout {
    brand: Result<String, NvmlFailureKind>,
    pci_bus_id: Result<String, NvmlFailureKind>,
    uuid: Result<String, NvmlFailureKind>,
    utilization: Result<(u32, u32), NvmlFailureKind>,
    memory: Result<(u64, u64), NvmlFailureKind>,
    temperature_c: Result<f32, NvmlFailureKind>,
    power_w: Result<f32, NvmlFailureKind>,
    current_clock_mhz: Result<u64, NvmlFailureKind>,
    max_clock_mhz: Result<u64, NvmlFailureKind>,
    encoder_pct: Result<f32, NvmlFailureKind>,
    decoder_pct: Result<f32, NvmlFailureKind>,
    fan_speed_pct: Result<f32, NvmlFailureKind>,
    throttle_reasons: Result<Vec<GpuThrottleReason>, NvmlFailureKind>,
}

#[cfg(feature = "nvidia")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NvmlFieldFailure {
    field: GpuMetricField,
    kind: NvmlFailureKind,
}

#[cfg(feature = "nvidia")]
#[derive(Debug)]
struct NvmlDeviceAssembly {
    sample: Option<GpuProviderSample>,
    failures: Vec<NvmlFieldFailure>,
}

#[cfg(feature = "nvidia")]
fn read_nvml_device(device: &nvml_wrapper::Device<'_>) -> NvmlDeviceReadout {
    use nvml_wrapper::enum_wrappers::device::{Clock, TemperatureSensor};

    NvmlDeviceReadout {
        brand: device.name().map_err(|error| classify_error(&error)),
        pci_bus_id: device
            .pci_info()
            .map(|pci| pci.bus_id)
            .map_err(|error| classify_error(&error)),
        uuid: device.uuid().map_err(|error| classify_error(&error)),
        utilization: device
            .utilization_rates()
            .map(|utilization| (utilization.gpu, utilization.memory))
            .map_err(|error| classify_error(&error)),
        memory: device
            .memory_info()
            .map(|memory| (memory.used, memory.total))
            .map_err(|error| classify_error(&error)),
        temperature_c: device
            .temperature(TemperatureSensor::Gpu)
            .map(|temperature| temperature as f32)
            .map_err(|error| classify_error(&error)),
        power_w: device
            .power_usage()
            .map(|milliwatts| milliwatts as f32 / 1_000.0)
            .map_err(|error| classify_error(&error)),
        current_clock_mhz: device
            .clock_info(Clock::Graphics)
            .map(u64::from)
            .map_err(|error| classify_error(&error)),
        max_clock_mhz: device
            .max_clock_info(Clock::Graphics)
            .map(u64::from)
            .map_err(|error| classify_error(&error)),
        encoder_pct: device
            .encoder_utilization()
            .map(|info| (info.utilization as f32).clamp(0.0, 100.0))
            .map_err(|error| classify_error(&error)),
        decoder_pct: device
            .decoder_utilization()
            .map(|info| (info.utilization as f32).clamp(0.0, 100.0))
            .map_err(|error| classify_error(&error)),
        fan_speed_pct: read_maximum_fan_speed(device),
        throttle_reasons: read_throttle_reasons(device),
    }
}

#[cfg(feature = "nvidia")]
fn read_maximum_fan_speed(device: &nvml_wrapper::Device<'_>) -> Result<f32, NvmlFailureKind> {
    let count = device.num_fans().map_err(|error| classify_error(&error))?;
    if count == 0 {
        return Err(NvmlFailureKind::NotSupported);
    }
    let readings = (0..count)
        .map(|index| {
            device
                .fan_speed(index)
                .map(|speed| speed as f32)
                .map_err(|error| classify_error(&error))
        })
        .collect::<Vec<_>>();
    select_maximum_fan_speed(&readings)
}

#[cfg(feature = "nvidia")]
fn select_maximum_fan_speed(
    readings: &[Result<f32, NvmlFailureKind>],
) -> Result<f32, NvmlFailureKind> {
    let successful = readings
        .iter()
        .filter_map(|reading| reading.as_ref().ok().copied())
        .collect::<Vec<_>>();
    if successful.is_empty() {
        return Err(preferred_failure(
            &readings
                .iter()
                .filter_map(|reading| reading.as_ref().err().copied())
                .collect::<Vec<_>>(),
        ));
    }
    successful
        .into_iter()
        .reduce(f32::max)
        .map(|speed| speed.clamp(0.0, 100.0))
        .ok_or(NvmlFailureKind::NotSupported)
}

#[cfg(feature = "nvidia")]
fn assemble_nvml_device(readout: NvmlDeviceReadout) -> NvmlDeviceAssembly {
    let mut failures = Vec::new();
    let pci_id = match readout.pci_bus_id {
        Ok(bus_id) => normalize_pci_slot(&bus_id)
            .map(|slot| stable_gpu_id("nvidia", Some(&slot)))
            .or_else(|| {
                failures.push(NvmlFieldFailure {
                    field: GpuMetricField::Identity,
                    kind: NvmlFailureKind::Transient,
                });
                None
            }),
        Err(kind) => {
            failures.push(NvmlFieldFailure {
                field: GpuMetricField::Identity,
                kind,
            });
            None
        }
    };
    let uuid_id = match readout.uuid {
        Ok(uuid) if !uuid.trim().is_empty() => Some(format!("gpu:uuid:{}", uuid.trim())),
        Ok(_) => {
            failures.push(NvmlFieldFailure {
                field: GpuMetricField::Identity,
                kind: NvmlFailureKind::Transient,
            });
            None
        }
        Err(kind) => {
            failures.push(NvmlFieldFailure {
                field: GpuMetricField::Identity,
                kind,
            });
            None
        }
    };
    let Some(device_id) = pci_id.or(uuid_id) else {
        return NvmlDeviceAssembly {
            sample: None,
            failures,
        };
    };

    let mut metrics = GpuMetrics::new(device_id, "");
    metrics.device_state = DeviceState {
        status: DeviceStatus::Healthy,
        last_success_ms: None,
    };
    let mut observations = GpuScalarObservations::default();
    let mut fields = vec![GpuMetricField::Identity];

    match readout.brand {
        Ok(brand) if !brand.trim().is_empty() => {
            metrics.brand = brand;
            fields.push(GpuMetricField::Brand);
        }
        Ok(_) => failures.push(NvmlFieldFailure {
            field: GpuMetricField::Brand,
            kind: NvmlFailureKind::Transient,
        }),
        Err(kind) => failures.push(NvmlFieldFailure {
            field: GpuMetricField::Brand,
            kind,
        }),
    }
    match readout.utilization {
        Ok((gpu, _memory)) => {
            let utilization = (gpu as f32).clamp(0.0, 100.0);
            observations.utilization_pct = ScalarObservation::available(utilization, 0);
            fields.push(GpuMetricField::Utilization);
        }
        Err(kind) => failures.push(NvmlFieldFailure {
            field: GpuMetricField::Utilization,
            kind,
        }),
    }
    match readout.memory {
        Ok((used, total)) => {
            observations.memory_used_bytes = ScalarObservation::available(used, 0);
            observations.memory_total_bytes = ScalarObservation::available(total, 0);
            observations.dedicated_vram_used_bytes = ScalarObservation::available(used, 0);
            observations.dedicated_vram_total_bytes = ScalarObservation::available(total, 0);
            fields.push(GpuMetricField::Memory);
        }
        Err(kind) => failures.push(NvmlFieldFailure {
            field: GpuMetricField::Memory,
            kind,
        }),
    }
    match readout.temperature_c {
        Ok(temperature) => {
            observations.temperature_c = ScalarObservation::available(temperature, 0);
            fields.push(GpuMetricField::Temperature);
        }
        Err(kind) => failures.push(NvmlFieldFailure {
            field: GpuMetricField::Temperature,
            kind,
        }),
    }
    match readout.power_w {
        Ok(power) => {
            observations.power_w = ScalarObservation::available(power, 0);
            fields.push(GpuMetricField::Power);
        }
        Err(kind) => failures.push(NvmlFieldFailure {
            field: GpuMetricField::Power,
            kind,
        }),
    }

    let mut engines = Vec::new();
    match readout.encoder_pct {
        Ok(usage_pct) => engines.push(GpuEngine {
            name: "Video Encode".to_string(),
            kind: GpuEngineKind::VideoEncode,
            usage_pct,
        }),
        Err(kind) => failures.push(NvmlFieldFailure {
            field: GpuMetricField::Engines,
            kind,
        }),
    }
    match readout.decoder_pct {
        Ok(usage_pct) => engines.push(GpuEngine {
            name: "Video Decode".to_string(),
            kind: GpuEngineKind::VideoDecode,
            usage_pct,
        }),
        Err(kind) => failures.push(NvmlFieldFailure {
            field: GpuMetricField::Engines,
            kind,
        }),
    }
    if !engines.is_empty() {
        metrics.engines = engines;
        fields.push(GpuMetricField::Engines);
    }

    match readout.fan_speed_pct {
        Ok(speed) => {
            observations.fan_speed_pct = ScalarObservation::available(speed, 0);
            fields.push(GpuMetricField::Fan);
        }
        Err(kind) => failures.push(NvmlFieldFailure {
            field: GpuMetricField::Fan,
            kind,
        }),
    }
    match readout.current_clock_mhz {
        Ok(clock) => observations.frequency_mhz = ScalarObservation::available(clock, 0),
        Err(kind) => failures.push(NvmlFieldFailure {
            field: GpuMetricField::Frequency,
            kind,
        }),
    }
    match readout.max_clock_mhz {
        Ok(clock) => observations.max_frequency_mhz = ScalarObservation::available(clock, 0),
        Err(kind) => failures.push(NvmlFieldFailure {
            field: GpuMetricField::Frequency,
            kind,
        }),
    }
    if observations.frequency_mhz.current_value().is_some()
        || observations.max_frequency_mhz.current_value().is_some()
    {
        fields.push(GpuMetricField::Frequency);
    }
    match readout.throttle_reasons {
        Ok(reasons) => {
            metrics.apply_throttle_observation(ScalarObservation::available(reasons, 0));
            fields.push(GpuMetricField::Throttle);
        }
        Err(kind) => failures.push(NvmlFieldFailure {
            field: GpuMetricField::Throttle,
            kind,
        }),
    }

    let field_failures = failures
        .iter()
        .map(|failure| GpuProviderFieldFailure {
            field: failure.field,
            failure: failure.kind.failure_kind(),
        })
        .collect();
    metrics.apply_scalar_observations(observations);
    NvmlDeviceAssembly {
        sample: Some(GpuProviderSample {
            metrics,
            fields,
            field_failures,
        }),
        failures,
    }
}

#[cfg(feature = "nvidia")]
fn preferred_failure(failures: &[NvmlFailureKind]) -> NvmlFailureKind {
    [
        NvmlFailureKind::PermissionDenied,
        NvmlFailureKind::MissingLibrary,
        NvmlFailureKind::Transient,
        NvmlFailureKind::NotSupported,
        NvmlFailureKind::Unsupported,
    ]
    .into_iter()
    .find(|candidate| failures.contains(candidate))
    .unwrap_or(NvmlFailureKind::Unsupported)
}

#[cfg(all(test, feature = "nvidia"))]
#[path = "../../../../tests/headless/linux_engine_hardware_gpu_nvidia_tests.rs"]
mod tests;
