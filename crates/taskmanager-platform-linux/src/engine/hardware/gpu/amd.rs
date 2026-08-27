//! Runtime AMDGPU sysfs/hwmon enrichment for the generic DRM inventory.

use std::collections::BTreeMap;
use std::ffi::OsStr;

use super::*;

#[derive(Debug, Default)]
pub(super) struct AmdDeviceProbe {
    pub(super) is_amd: bool,
    pub(super) sample: Option<GpuProviderSample>,
}

/// Probe one DRM device without any PCI/SKU allowlist.
///
/// Runtime vendor/driver markers select the provider. Every metric remains
/// optional and is advertised through `fields` only when its kernel node was
/// actually readable.
pub(super) fn probe_amdgpu_device(_card_name: &str, device_path: &Path) -> AmdDeviceProbe {
    let driver = read_driver_name(device_path);
    let vendor = read_text_field(&device_path.join("vendor"), false);
    let is_amd = driver.as_deref() == Some("amdgpu")
        || vendor
            .value
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("0x1002"));
    if !is_amd {
        return AmdDeviceProbe::default();
    }

    let mut metric = GpuMetrics::new(
        linux_gpu_device_id(device_path, read_pci_slot_name(device_path).as_deref()),
        "",
    );
    metric.device_state = DeviceState {
        status: DeviceStatus::Healthy,
        last_success_ms: None,
    };
    let mut observations = GpuScalarObservations::default();
    let mut fields = Vec::new();
    let mut failures = BTreeMap::new();

    let utilization = read_text_field(&device_path.join("gpu_busy_percent"), false);
    if let Some(usage_pct) = utilization.value.as_deref().and_then(parse_busy_percent) {
        observations.utilization_pct = ScalarObservation::available(usage_pct, 0);
        fields.push(GpuMetricField::Utilization);
    } else if utilization.value.is_some() {
        record_failure(
            &mut failures,
            GpuMetricField::Utilization,
            FailureKind::ProviderFault,
        );
    }
    record_optional_failure(
        &mut failures,
        GpuMetricField::Utilization,
        utilization.failure,
    );

    let engines = read_amdgpu_engines_detailed(device_path);
    metric.engines = engines.value.unwrap_or_default();
    if !metric.engines.is_empty() {
        fields.push(GpuMetricField::Engines);
    }
    record_optional_failure(&mut failures, GpuMetricField::Engines, engines.failure);

    let dedicated_used = read_u64_field(&device_path.join("mem_info_vram_used"), false);
    let dedicated_total = read_u64_field(&device_path.join("mem_info_vram_total"), false);
    let shared_used = read_u64_field(&device_path.join("mem_info_gtt_used"), false);
    let shared_total = read_u64_field(&device_path.join("mem_info_gtt_total"), false);
    for failure in [
        dedicated_used.failure,
        dedicated_total.failure,
        shared_used.failure,
        shared_total.failure,
    ] {
        record_optional_failure(&mut failures, GpuMetricField::Memory, failure);
    }
    let dedicated_used = dedicated_used.value;
    let dedicated_total = dedicated_total.value;
    let shared_used = shared_used.value;
    let shared_total = shared_total.value;
    // ADR-015: each `mem_info_*` sysfs node is independently fallible. Publish
    // only the specific reading that actually succeeded; a missing sibling must
    // surface through the typed `memory_*_bytes` Option plus the per-field
    // `GpuMetricField::Memory` failure recorded above, never by collapsing to a
    // believable zero. Filling an unread total with `unwrap_or_default()` made
    // the device report "0 bytes total VRAM" and could invert `used > total`.
    // Each successful node becomes its own typed fact; a missing sibling stays
    // Unknown here and is converted to a typed failure by the registry receipt.
    let mut memory_observed = false;
    if let Some(bytes) = dedicated_used {
        observations.dedicated_vram_used_bytes = ScalarObservation::available(bytes, 0);
        memory_observed = true;
    }
    if let Some(bytes) = dedicated_total {
        observations.dedicated_vram_total_bytes = ScalarObservation::available(bytes, 0);
        memory_observed = true;
    }
    if let Some(bytes) = shared_used {
        observations.shared_vram_used_bytes = ScalarObservation::available(bytes, 0);
        memory_observed = true;
    }
    if let Some(bytes) = shared_total {
        observations.shared_vram_total_bytes = ScalarObservation::available(bytes, 0);
        memory_observed = true;
    }
    if let Some(bytes) = optional_sum(dedicated_used, shared_used) {
        observations.memory_used_bytes = ScalarObservation::available(bytes, 0);
    }
    if let Some(bytes) = optional_sum(dedicated_total, shared_total) {
        observations.memory_total_bytes = ScalarObservation::available(bytes, 0);
    }
    if memory_observed {
        fields.push(GpuMetricField::Memory);
    }

    let hwmon_dirs = read_hwmon_dirs(device_path);
    let hwmon_failure = hwmon_dirs.failure;
    let hwmon_dirs = hwmon_dirs.value.unwrap_or_default();
    let temperature = read_numbered_hwmon_value(&hwmon_dirs, "temp", "_input", hwmon_failure);
    if let Some(millidegrees) = temperature.value {
        let temperature = millidegrees as f32 / 1_000.0;
        observations.temperature_c = ScalarObservation::available(temperature, 0);
        fields.push(GpuMetricField::Temperature);
    }
    record_optional_failure(
        &mut failures,
        GpuMetricField::Temperature,
        temperature.failure,
    );
    let power = read_named_hwmon_value(
        &hwmon_dirs,
        &["power1_input", "power1_average"],
        hwmon_failure,
    );
    if let Some(microwatts) = power.value {
        observations.power_w = ScalarObservation::available(microwatts as f32 / 1_000_000.0, 0);
        fields.push(GpuMetricField::Power);
    }
    record_optional_failure(&mut failures, GpuMetricField::Power, power.failure);

    let fan_rpm = read_numbered_hwmon_value(&hwmon_dirs, "fan", "_input", hwmon_failure);
    let fan_pct = read_pwm_percent(&hwmon_dirs, hwmon_failure);
    if let Some(value) = fan_rpm.value {
        observations.fan_speed_rpm = ScalarObservation::available(value, 0);
    }
    if let Some(value) = fan_pct.value {
        observations.fan_speed_pct = ScalarObservation::available(value, 0);
    }
    if fan_rpm.value.is_some() || fan_pct.value.is_some() {
        fields.push(GpuMetricField::Fan);
    }
    record_optional_failure(&mut failures, GpuMetricField::Fan, fan_rpm.failure);
    record_optional_failure(&mut failures, GpuMetricField::Fan, fan_pct.failure);

    let dpm_text = read_text_field(&device_path.join("pp_dpm_sclk"), false);
    let dpm_clock = dpm_text
        .value
        .as_deref()
        .map(parse_amdgpu_dpm_clock)
        .unwrap_or_default();
    let dpm_failure = if dpm_text.value.is_some() && dpm_clock == (None, None) {
        Some(FailureKind::ProviderFault)
    } else {
        dpm_text.failure
    };
    let hwmon_clock = read_named_hwmon_value(&hwmon_dirs, &["freq1_input"], hwmon_failure);
    let frequency_mhz = dpm_clock
        .0
        .or_else(|| hwmon_clock.value.map(|hertz| hertz / 1_000_000))
        .filter(|mhz| *mhz > 0);
    let max_frequency_mhz = dpm_clock.1.filter(|mhz| *mhz > 0);
    if let Some(value) = frequency_mhz {
        observations.frequency_mhz = ScalarObservation::available(value, 0);
    }
    if let Some(value) = max_frequency_mhz {
        observations.max_frequency_mhz = ScalarObservation::available(value, 0);
    }
    if frequency_mhz.is_some() || max_frequency_mhz.is_some() {
        fields.push(GpuMetricField::Frequency);
    }
    record_optional_failure(&mut failures, GpuMetricField::Frequency, dpm_failure);
    if frequency_mhz.is_none() {
        record_optional_failure(
            &mut failures,
            GpuMetricField::Frequency,
            hwmon_clock.failure,
        );
    }

    let throttle = read_throttle_status(device_path, &hwmon_dirs);
    if let Some(throttle) = throttle.value {
        let reasons = (!throttle.is_empty())
            .then_some(vec![GpuThrottleReason::Other])
            .unwrap_or_default();
        metric.apply_throttle_observation(ScalarObservation::available(reasons, 0));
        fields.push(GpuMetricField::Throttle);
    }
    record_optional_failure(&mut failures, GpuMetricField::Throttle, throttle.failure);
    metric.apply_scalar_observations(observations);

    AmdDeviceProbe {
        is_amd,
        sample: Some(GpuProviderSample {
            metrics: metric,
            fields,
            field_failures: failures
                .into_iter()
                .map(|(field, failure)| GpuProviderFieldFailure { field, failure })
                .collect(),
        }),
    }
}

/// Dynamically enumerate every readable `*_busy_percent` engine node.
///
/// The aggregate `gpu_busy_percent` and memory `mem_busy_percent` nodes are not
/// engines. Unknown future engine names are retained with a deterministic
/// label rather than dropped behind a device/SKU list.
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", allow(dead_code))]
pub(super) fn read_amdgpu_engines(device_path: &Path) -> Vec<GpuEngine> {
    read_amdgpu_engines_detailed(device_path)
        .value
        .unwrap_or_default()
}

fn read_amdgpu_engines_detailed(device_path: &Path) -> GpuFieldRead<Vec<GpuEngine>> {
    let discovered = read_directory_paths(device_path, false);
    let mut failure = discovered.failure;
    let mut nodes = discovered
        .value
        .unwrap_or_default()
        .into_iter()
        .filter_map(|path| {
            let engine = path
                .file_name()?
                .to_str()?
                .strip_suffix("_busy_percent")?
                .to_string();
            (!matches!(engine.as_str(), "gpu" | "mem")).then_some((engine, path))
        })
        .collect::<Vec<_>>();
    nodes.sort_by(|left, right| left.0.cmp(&right.0));
    if nodes.is_empty() {
        return GpuFieldRead::unavailable(failure.unwrap_or(FailureKind::Unsupported));
    }

    let mut engines = Vec::new();
    for (engine, path) in nodes {
        let raw = read_text_field(&path, true);
        failure = preferred_failure(failure, raw.failure);
        match raw.value.as_deref().and_then(parse_busy_percent) {
            Some(usage_pct) => {
                let name = engine_label(&engine);
                engines.push(GpuEngine {
                    kind: GpuEngineKind::from_display_name(&name),
                    name,
                    usage_pct,
                });
            }
            None if raw.value.is_some() => {
                failure = preferred_failure(failure, Some(FailureKind::ProviderFault));
            }
            None => {}
        }
    }
    field_read(engines, failure)
}

fn engine_label(engine: &str) -> String {
    match engine {
        "gfx" => "Graphics (3D)".to_string(),
        "compute" => "Compute".to_string(),
        "sdma" => "Memory (Copy)".to_string(),
        "dec" => "Video Decode".to_string(),
        "enc" => "Video Encode".to_string(),
        "uhd" => "UHD".to_string(),
        "jpeg" => "JPEG".to_string(),
        "vcn" => "VCN".to_string(),
        "vce" => "VCE".to_string(),
        other => other.replace('_', " ").to_ascii_uppercase(),
    }
}

pub(super) fn parse_amdgpu_dpm_clock(text: &str) -> (Option<u64>, Option<u64>) {
    let mut current = None;
    let mut maximum: Option<u64> = None;
    for line in text.lines() {
        let frequency = line.split_whitespace().find_map(parse_mhz_token);
        if let Some(frequency) = frequency {
            maximum = Some(maximum.map_or(frequency, |known| known.max(frequency)));
            if line.contains('*') {
                current = Some(frequency);
            }
        }
    }
    (current, maximum)
}

fn parse_mhz_token(token: &str) -> Option<u64> {
    let token = token.trim_matches(|character: char| character == ',' || character == '*');
    let lowercase = token.to_ascii_lowercase();
    lowercase.strip_suffix("mhz")?.parse().ok()
}

fn optional_sum(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (None, None) => None,
        (left, right) => Some(
            left.unwrap_or_default()
                .saturating_add(right.unwrap_or_default()),
        ),
    }
}

fn read_hwmon_dirs(device_path: &Path) -> GpuFieldRead<Vec<PathBuf>> {
    let mut read = read_directory_paths(&device_path.join("hwmon"), false);
    if let Some(paths) = &mut read.value {
        paths.sort();
        if paths.is_empty() {
            return GpuFieldRead::unavailable(FailureKind::Unsupported);
        }
    }
    read
}

fn read_directory_paths(path: &Path, discovered: bool) -> GpuFieldRead<Vec<PathBuf>> {
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) => return GpuFieldRead::unavailable(io_failure(&error, discovered)),
    };
    let mut paths = Vec::new();
    let mut failure = None;
    for entry in entries {
        match entry {
            Ok(entry) => paths.push(entry.path()),
            Err(error) => {
                failure = preferred_failure(failure, Some(io_failure(&error, true)));
            }
        }
    }
    field_read(paths, failure)
}

fn read_text_field(path: &Path, discovered: bool) -> GpuFieldRead<String> {
    match fs::read_to_string(path) {
        Ok(value) => {
            let value = value.trim().to_string();
            if value.is_empty() {
                GpuFieldRead::unavailable(FailureKind::ProviderFault)
            } else {
                GpuFieldRead::available(value)
            }
        }
        Err(error) => GpuFieldRead::unavailable(io_failure(&error, discovered)),
    }
}

fn read_u64_field(path: &Path, discovered: bool) -> GpuFieldRead<u64> {
    let text = read_text_field(path, discovered);
    match text.value {
        Some(value) => value.parse().map_or_else(
            |_| GpuFieldRead::unavailable(FailureKind::ProviderFault),
            |value| match text.failure {
                Some(failure) => GpuFieldRead::partial(value, failure),
                None => GpuFieldRead::available(value),
            },
        ),
        None => GpuFieldRead::unavailable(text.failure.unwrap_or(FailureKind::ProviderFault)),
    }
}

fn read_named_hwmon_value(
    hwmon_dirs: &[PathBuf],
    names: &[&str],
    root_failure: Option<FailureKind>,
) -> GpuFieldRead<u64> {
    let mut failure = root_failure;
    for directory in hwmon_dirs {
        for name in names {
            let read = read_u64_field(&directory.join(name), false);
            if let Some(value) = read.value {
                return GpuFieldRead::available(value);
            }
            failure = preferred_failure(failure, read.failure);
        }
    }
    GpuFieldRead::unavailable(failure.unwrap_or(FailureKind::Unsupported))
}

fn read_numbered_hwmon_value(
    hwmon_dirs: &[PathBuf],
    prefix: &str,
    suffix: &str,
    root_failure: Option<FailureKind>,
) -> GpuFieldRead<u64> {
    let mut failure = root_failure;
    let mut nodes = Vec::new();
    for directory in hwmon_dirs {
        let discovered = read_directory_paths(directory, true);
        failure = preferred_failure(failure, discovered.failure);
        nodes.extend(
            discovered
                .value
                .unwrap_or_default()
                .into_iter()
                .filter(|path| {
                    path.file_name()
                        .and_then(OsStr::to_str)
                        .and_then(|name| name.strip_prefix(prefix))
                        .and_then(|name| name.strip_suffix(suffix))
                        .is_some_and(|index| {
                            !index.is_empty() && index.chars().all(|char| char.is_ascii_digit())
                        })
                }),
        );
    }
    nodes.sort();
    if nodes.is_empty() {
        return GpuFieldRead::unavailable(failure.unwrap_or(FailureKind::Unsupported));
    }
    for path in nodes {
        let read = read_u64_field(&path, true);
        if let Some(value) = read.value {
            return match preferred_failure(failure, read.failure) {
                Some(failure) => GpuFieldRead::partial(value, failure),
                None => GpuFieldRead::available(value),
            };
        }
        failure = preferred_failure(failure, read.failure);
    }
    GpuFieldRead::unavailable(failure.unwrap_or(FailureKind::Unsupported))
}

fn read_pwm_percent(
    hwmon_dirs: &[PathBuf],
    root_failure: Option<FailureKind>,
) -> GpuFieldRead<f32> {
    let mut failure = root_failure;
    let mut nodes = Vec::new();
    for directory in hwmon_dirs {
        let discovered = read_directory_paths(directory, true);
        failure = preferred_failure(failure, discovered.failure);
        nodes.extend(
            discovered
                .value
                .unwrap_or_default()
                .into_iter()
                .filter_map(|path| {
                    let index = path
                        .file_name()
                        .and_then(OsStr::to_str)?
                        .strip_prefix("pwm")?;
                    (!index.is_empty() && index.chars().all(|char| char.is_ascii_digit()))
                        .then_some((index.to_string(), path))
                }),
        );
    }
    nodes.sort_by(|left, right| left.0.cmp(&right.0));
    if nodes.is_empty() {
        return GpuFieldRead::unavailable(failure.unwrap_or(FailureKind::Unsupported));
    }
    for (index, path) in nodes {
        let value = read_u64_field(&path, true);
        failure = preferred_failure(failure, value.failure);
        let Some(value) = value.value else {
            continue;
        };
        let maximum_path = path.with_file_name(format!("pwm{index}_max"));
        let maximum_read = read_u64_field(&maximum_path, false);
        let maximum = if maximum_read.failure == Some(FailureKind::Unsupported) {
            255
        } else if let Some(maximum) = maximum_read.value {
            maximum
        } else {
            failure = preferred_failure(failure, maximum_read.failure);
            continue;
        };
        if maximum == 0 || value > maximum {
            failure = preferred_failure(failure, Some(FailureKind::ProviderFault));
            continue;
        }
        let percent = (value as f32 / maximum as f32) * 100.0;
        return match failure {
            Some(failure) => GpuFieldRead::partial(percent, failure),
            None => GpuFieldRead::available(percent),
        };
    }
    GpuFieldRead::unavailable(failure.unwrap_or(FailureKind::Unsupported))
}

fn read_throttle_status(device_path: &Path, hwmon_dirs: &[PathBuf]) -> GpuFieldRead<String> {
    let mut roots = vec![device_path.to_path_buf()];
    roots.extend_from_slice(hwmon_dirs);
    let mut nodes = Vec::new();
    let mut failure = None;
    for root in roots {
        let discovered = read_directory_paths(&root, true);
        failure = preferred_failure(failure, discovered.failure);
        nodes.extend(
            discovered
                .value
                .unwrap_or_default()
                .into_iter()
                .filter(|path| {
                    path.file_name()
                        .and_then(OsStr::to_str)
                        .is_some_and(|name| name.contains("throttle") && name.ends_with("status"))
                }),
        );
    }
    nodes.sort();
    if nodes.is_empty() {
        return GpuFieldRead::unavailable(failure.unwrap_or(FailureKind::Unsupported));
    }
    for path in nodes {
        let raw = read_text_field(&path, true);
        failure = preferred_failure(failure, raw.failure);
        let Some(raw) = raw.value else {
            continue;
        };
        if parses_as_zero(&raw) {
            return field_read(String::new(), failure);
        }
        if parses_as_nonzero(&raw) {
            return field_read(raw, failure);
        }
        failure = preferred_failure(failure, Some(FailureKind::ProviderFault));
    }
    GpuFieldRead::unavailable(failure.unwrap_or(FailureKind::Unsupported))
}

fn parses_as_zero(value: &str) -> bool {
    value.parse::<u64>().ok() == Some(0)
        || value
            .strip_prefix("0x")
            .and_then(|hex| u64::from_str_radix(hex, 16).ok())
            == Some(0)
}

fn parses_as_nonzero(value: &str) -> bool {
    value.parse::<u64>().is_ok_and(|value| value > 0)
        || value
            .strip_prefix("0x")
            .and_then(|hex| u64::from_str_radix(hex, 16).ok())
            .is_some_and(|value| value > 0)
}

fn field_read<T>(value: T, failure: Option<FailureKind>) -> GpuFieldRead<T> {
    match failure {
        Some(failure) => GpuFieldRead::partial(value, failure),
        None => GpuFieldRead::available(value),
    }
}

fn record_optional_failure(
    failures: &mut BTreeMap<GpuMetricField, FailureKind>,
    field: GpuMetricField,
    failure: Option<FailureKind>,
) {
    if let Some(failure) = failure {
        record_failure(failures, field, failure);
    }
}

fn record_failure(
    failures: &mut BTreeMap<GpuMetricField, FailureKind>,
    field: GpuMetricField,
    failure: FailureKind,
) {
    failures
        .entry(field)
        .and_modify(|current| {
            *current = preferred_failure(Some(*current), Some(failure)).unwrap_or(failure);
        })
        .or_insert(failure);
}

fn preferred_failure(
    current: Option<FailureKind>,
    candidate: Option<FailureKind>,
) -> Option<FailureKind> {
    preferred_gpu_failure(current, candidate)
}

fn io_failure(error: &std::io::Error, discovered: bool) -> FailureKind {
    let missing = if discovered {
        FailureKind::TemporarilyUnavailable
    } else {
        FailureKind::Unsupported
    };
    gpu_io_failure(error, missing)
}

#[cfg(test)]
#[path = "../../../../tests/headless/linux_engine_hardware_gpu_amd_tests.rs"]
mod tests;
