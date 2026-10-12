//! macOS GPU inventory telemetry built on the safe `system_profiler` shell-out.
//!
//! `system_profiler SPDisplaysDataType -json` (run through the bounded command
//! runner) is the same safe source `MacHardwareInventoryProvider` uses for
//! model/chip facts. It publishes GPU model/brand and, for discrete adapters,
//! total VRAM, but no verified per-adapter identity. A bounded
//! `ioreg -r -c IOAccelerator -l` sample adds
//! overall device utilization when it can be unambiguously matched to a
//! single discovered adapter; temperature, power, frequency and fan remain
//! absent when the registry does not expose them (ADR-019).

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_core::{
    DEFAULT_DEVICE_ABSENCE_RETENTION_MS, DeviceId, DeviceLifecycle, DeviceLifecycleRegistry,
    DeviceRefreshOutcome, DeviceState, DeviceStatus, FailureKind, GpuMetricField,
    GpuMetricProvenance, GpuMetrics, GpuScalarObservations, GpuTelemetryObservation, ProviderId,
    ScalarObservation,
};
use taskmanager_platform_contract::ProviderFailure;
use taskmanager_platform_provider::GpuTelemetryProvider;

use taskmanager_platform_portable::{BoundedCommandError, run_with_timeout};

const GPU_TELEMETRY_PROVIDER: ProviderId = ProviderId::borrowed("macos.telemetry.gpu");
const GPU_IOREG_PROVIDER: ProviderId = ProviderId::borrowed("macos.telemetry.gpu.ioreg");

fn unavailable_source(provider: ProviderId, failure: FailureKind) -> SourceStatus {
    SourceStatus {
        provider,
        outcome: SourceOutcome::Unavailable(failure),
        item_count: 0,
    }
}

/// GPU inventory telemetry from `system_profiler SPDisplaysDataType -json`.
/// That source reports model/vendor facts but no verified per-adapter stable
/// identity, so its non-empty rows remain visible without a row identity or
/// lifecycle generation. A complete empty inventory can still confirm prior
/// identities absent. Live dynamic scalars have no safe macOS source except
/// optional `ioreg` utilization (ADR-019).
pub struct MacGpuTelemetryProvider {
    lifecycles: DeviceLifecycleRegistry,
}

impl MacGpuTelemetryProvider {
    #[must_use]
    pub fn new() -> Self {
        Self {
            lifecycles: DeviceLifecycleRegistry::new(DEFAULT_DEVICE_ABSENCE_RETENTION_MS),
        }
    }
}

impl Default for MacGpuTelemetryProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl GpuTelemetryProvider for MacGpuTelemetryProvider {
    fn refresh(&mut self, observed_at_ms: u64) -> Result<GpuTelemetryObservation, ProviderFailure> {
        let profiler_result = system_profiler_gpus();
        let dynamic_result = match &profiler_result {
            Ok(enumeration)
                if enumeration.failure.is_none()
                    && enumeration.raw_item_count == 1
                    && enumeration.adapters.len() == 1 =>
            {
                collect_ioreg_gpu_utilization()
            }
            _ => Ok(None),
        };
        Ok(gpu_observation_from_profiler_with_dynamic(
            &mut self.lifecycles,
            profiler_result,
            dynamic_result,
            observed_at_ms,
        ))
    }
}

fn gpu_observation_from_profiler_with_dynamic(
    lifecycles: &mut DeviceLifecycleRegistry,
    profiler_result: Result<MacGpuEnumeration, FailureKind>,
    dynamic_result: Result<Option<f32>, FailureKind>,
    observed_at_ms: u64,
) -> GpuTelemetryObservation {
    lifecycles.begin_refresh();
    let enumeration = match profiler_result {
        Ok(enumeration) => enumeration,
        Err(failure) => {
            let _delta = lifecycles.finish_refresh(
                DeviceRefreshOutcome::Unavailable(DeviceStatus::from_failure(failure)),
                observed_at_ms,
            );
            return GpuTelemetryObservation::unavailable(
                failure,
                vec![unavailable_source(GPU_TELEMETRY_PROVIDER, failure)],
                Vec::new(),
                lifecycle_snapshot(lifecycles),
            );
        }
    };
    let can_match_dynamic = enumeration.failure.is_none() && enumeration.raw_item_count == 1;
    let mut adapters = enumeration.adapters;
    adapters.sort_by(|left, right| left.identity.cmp(&right.identity));
    let adapter_count = adapters.len();
    let (dynamic_utilization, dynamic_failure) = match dynamic_result {
        Ok(value) => (value, None),
        Err(failure) => (None, Some(failure)),
    };
    let mut identity_counts = HashMap::<String, usize>::new();
    for identity in adapters
        .iter()
        .filter_map(|adapter| adapter.identity.as_deref())
        .map(str::trim)
        .filter(|identity| !identity.is_empty())
    {
        *identity_counts.entry(identity.to_string()).or_default() += 1;
    }

    let mut discovery_failure = enumeration.failure;
    let mut gpus = Vec::with_capacity(adapters.len());
    for adapter in adapters {
        let identity = adapter
            .identity
            .as_deref()
            .map(str::trim)
            .filter(|identity| !identity.is_empty())
            .filter(|identity| identity_counts.get(*identity) == Some(&1));
        if identity.is_none() {
            discovery_failure.get_or_insert(FailureKind::Unsupported);
        }
        let mut provenance = vec![GpuMetricProvenance {
            field: GpuMetricField::Brand,
            provider: GPU_TELEMETRY_PROVIDER,
        }];
        let device_id = identity.map(|identity| format!("macos:gpu:system-profiler:{identity}"));
        let mut row = GpuMetrics::new(device_id.as_deref().unwrap_or_default(), adapter.brand);
        row.device_state = DeviceState::healthy(observed_at_ms);
        if let Some(device_id) = device_id.as_deref() {
            let lifecycle = lifecycles.observe(
                device_id,
                DeviceState::healthy(observed_at_ms),
                observed_at_ms,
            );
            row.device_generation = lifecycle.generation;
        }
        let mut observations = GpuScalarObservations::default();
        if can_match_dynamic
            && adapter_count == 1
            && let Some(value) = dynamic_utilization
        {
            observations.utilization_pct = ScalarObservation::available(value, observed_at_ms);
            provenance.push(GpuMetricProvenance {
                field: GpuMetricField::Utilization,
                provider: GPU_IOREG_PROVIDER,
            });
        }
        if let Some(vram_bytes) = adapter.vram_total_bytes {
            observations.memory_total_bytes =
                ScalarObservation::available(vram_bytes, observed_at_ms);
            observations.dedicated_vram_total_bytes =
                ScalarObservation::available(vram_bytes, observed_at_ms);
            provenance.push(GpuMetricProvenance {
                field: GpuMetricField::Memory,
                provider: GPU_TELEMETRY_PROVIDER,
            });
            provenance.push(GpuMetricProvenance {
                field: GpuMetricField::DedicatedVram,
                provider: GPU_TELEMETRY_PROVIDER,
            });
        }
        row.apply_scalar_observations(observations);
        row.apply_throttle_observation(ScalarObservation::unavailable(FailureKind::Unsupported));
        row.provenance = provenance;
        gpus.push(row);
    }

    let discovery_outcome = if gpus.is_empty() && discovery_failure.is_none() {
        SourceOutcome::Empty
    } else if let Some(failure) = discovery_failure {
        SourceOutcome::Partial(failure)
    } else {
        SourceOutcome::Available
    };
    let discovery_refresh = match discovery_outcome {
        SourceOutcome::Available | SourceOutcome::Empty => DeviceRefreshOutcome::Complete,
        SourceOutcome::Partial(failure) | SourceOutcome::Unavailable(failure) => {
            DeviceRefreshOutcome::Unavailable(DeviceStatus::from_failure(failure))
        }
    };
    let _delta = lifecycles.finish_refresh(discovery_refresh, observed_at_ms);
    let mut sources = vec![SourceStatus {
        provider: GPU_TELEMETRY_PROVIDER,
        outcome: discovery_outcome,
        item_count: gpus.len(),
    }];
    if let Some(failure) = dynamic_failure {
        sources.push(unavailable_source(GPU_IOREG_PROVIDER, failure));
    } else if dynamic_utilization.is_some() && can_match_dynamic && adapter_count == 1 {
        sources.push(SourceStatus {
            provider: GPU_IOREG_PROVIDER,
            outcome: SourceOutcome::Available,
            item_count: 1,
        });
    }
    sources.sort_by(|left, right| left.provider.cmp(&right.provider));
    let lifecycles = lifecycle_snapshot(lifecycles);

    match (discovery_failure, dynamic_failure) {
        (Some(failure), _) => GpuTelemetryObservation::partial(
            gpus,
            observed_at_ms,
            failure,
            sources,
            Vec::new(),
            lifecycles,
        ),
        (None, Some(failure)) => GpuTelemetryObservation::partial(
            gpus,
            observed_at_ms,
            failure,
            sources,
            Vec::new(),
            lifecycles,
        ),
        (None, None) => {
            GpuTelemetryObservation::current(gpus, observed_at_ms, sources, Vec::new(), lifecycles)
        }
    }
}

fn lifecycle_snapshot(lifecycles: &DeviceLifecycleRegistry) -> BTreeMap<DeviceId, DeviceLifecycle> {
    lifecycles
        .iter()
        .map(|(id, lifecycle)| (DeviceId::new(id), *lifecycle))
        .collect()
}

/// Parsed profiler inventory. `failure` means the command succeeded but at
/// least one non-empty row could not be represented completely; callers may
/// keep valid rows while refusing to confirm absence for omitted devices.
struct MacGpuEnumeration {
    adapters: Vec<MacGpuAdapter>,
    failure: Option<FailureKind>,
    raw_item_count: usize,
}

/// One GPU adapter discovered by `system_profiler SPDisplaysDataType -json`.
struct MacGpuAdapter {
    /// A per-adapter identity only when the source has proved one. The current
    /// `system_profiler` display schema has no such field; vendor/device IDs,
    /// revision, gMux and brand describe a model or controller, not one unit.
    identity: Option<String>,
    brand: String,
    /// Total dedicated VRAM when system_profiler reports it (discrete GPUs).
    /// `None` on Apple Silicon unified-memory parts where system_profiler
    /// publishes no `spdisplays_vram` field.
    vram_total_bytes: Option<u64>,
}

/// Map one `SPDisplaysDataType` JSON row onto displayable adapter facts.
/// Stable identity is intentionally not derived from the model, PCI model IDs,
/// or row ordering.
fn gpu_adapter_from_json(row: &serde_json::Value) -> Option<MacGpuAdapter> {
    let brand = ["_name", "sppci_model"]
        .into_iter()
        .filter_map(|key| row.get(key).and_then(serde_json::Value::as_str))
        .map(str::trim)
        .find(|name| !name.is_empty())?
        .to_string();
    let vram_total_bytes = row
        .get("spdisplays_vram")
        .and_then(serde_json::Value::as_str)
        .and_then(parse_memory_string);
    Some(MacGpuAdapter {
        identity: None,
        brand,
        vram_total_bytes,
    })
}

/// `system_profiler SPDisplaysDataType -json` → one entry per top-level GPU
/// adapter. Invocation failures remain distinct from a successful empty
/// inventory.
fn system_profiler_gpus() -> Result<MacGpuEnumeration, FailureKind> {
    let mut command = std::process::Command::new("system_profiler");
    command.args(["SPDisplaysDataType", "-json"]);
    let output = match run_with_timeout(&mut command, Duration::from_secs(5)) {
        Ok(output) => output,
        Err(BoundedCommandError::Spawn(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(FailureKind::MissingDependency);
        }
        Err(BoundedCommandError::Spawn(error))
            if error.kind() == std::io::ErrorKind::PermissionDenied =>
        {
            return Err(FailureKind::PermissionDenied);
        }
        Err(BoundedCommandError::TimedOut | BoundedCommandError::ReaderTimedOut) => {
            return Err(FailureKind::TimedOut);
        }
        Err(
            BoundedCommandError::Spawn(_)
            | BoundedCommandError::ReaderStart(_)
            | BoundedCommandError::ReaderFailed
            | BoundedCommandError::ProcessTree
            | BoundedCommandError::OutputTooLarge,
        ) => return Err(FailureKind::ProviderFault),
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
        return Err(if stderr.contains("permission denied") {
            FailureKind::PermissionDenied
        } else {
            FailureKind::TemporarilyUnavailable
        });
    };
    parse_system_profiler_gpus(&output.stdout)
}

/// Read the overall GPU busy percentage from the public diagnostic view of
/// the macOS IORegistry. The command is bounded and the value is only
/// promoted when exactly one usable device percentage is present; multiple
/// adapters are left unassigned rather than paired by enumeration order.
fn collect_ioreg_gpu_utilization() -> Result<Option<f32>, FailureKind> {
    #[cfg(target_os = "macos")]
    {
        let mut command = std::process::Command::new("ioreg");
        command.args(["-r", "-c", "IOAccelerator", "-d", "2", "-l"]);
        let output = match run_with_timeout(&mut command, Duration::from_secs(2)) {
            Ok(output) => output,
            Err(BoundedCommandError::Spawn(error))
                if error.kind() == std::io::ErrorKind::NotFound =>
            {
                return Err(FailureKind::MissingDependency);
            }
            Err(BoundedCommandError::Spawn(error))
                if error.kind() == std::io::ErrorKind::PermissionDenied =>
            {
                return Err(FailureKind::PermissionDenied);
            }
            Err(BoundedCommandError::TimedOut | BoundedCommandError::ReaderTimedOut) => {
                return Err(FailureKind::TimedOut);
            }
            Err(_) => return Err(FailureKind::ProviderFault),
        };
        if !output.status.success() {
            return Err(FailureKind::TemporarilyUnavailable);
        }
        let values = parse_ioreg_gpu_utilizations(&String::from_utf8_lossy(&output.stdout));
        if values.len() == 1 {
            Ok(Some(values[0]))
        } else {
            Ok(None)
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(None)
    }
}

/// Extract overall utilization fields from ioreg's property-dictionary text.
/// `Device Utilization %` is preferred; older Intel drivers may only expose
/// `GPU Activity(%)`. Every returned value is finite and bounded.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn parse_ioreg_gpu_utilizations(output: &str) -> Vec<f32> {
    let primary = parse_ioreg_key_values(output, "\"Device Utilization %\"");
    if !primary.is_empty() {
        return primary;
    }
    parse_ioreg_key_values(output, "\"GPU Activity(%)\"")
        .into_iter()
        .chain(parse_ioreg_key_values(output, "\"Renderer Utilization %\""))
        .collect()
}

#[cfg(any(target_os = "macos", test))]
fn parse_ioreg_key_values(output: &str, key: &str) -> Vec<f32> {
    let mut values = Vec::new();
    for line in output.lines() {
        if !line.contains(key) {
            continue;
        }
        let Some((_, raw)) = line
            .split_once(key)
            .and_then(|(_, tail)| tail.split_once('='))
        else {
            continue;
        };
        let token = raw
            .trim()
            .trim_start_matches('=')
            .trim()
            .split([',', '}'])
            .next()
            .unwrap_or_default()
            .split_whitespace()
            .next()
            .unwrap_or_default();
        let Ok(value) = token.parse::<f32>() else {
            continue;
        };
        if value.is_finite() && (0.0..=100.0).contains(&value) {
            values.push(value);
        }
    }
    values
}

fn parse_system_profiler_gpus(output: &[u8]) -> Result<MacGpuEnumeration, FailureKind> {
    let root: serde_json::Value =
        serde_json::from_slice(output).map_err(|_| FailureKind::ProviderFault)?;
    let rows = root
        .get("SPDisplaysDataType")
        .and_then(serde_json::Value::as_array)
        .ok_or(FailureKind::ProviderFault)?;
    let mut adapters = Vec::with_capacity(rows.len());
    let mut failure = None;
    for row in rows {
        if let Some(adapter) = gpu_adapter_from_json(row) {
            adapters.push(adapter);
        } else {
            // A non-empty unrecognized row may represent a GPU that this
            // parser cannot report. Preserve recognized rows but never turn
            // the filtered result into an authoritative empty inventory.
            failure.get_or_insert(FailureKind::ProviderFault);
        }
    }
    Ok(MacGpuEnumeration {
        adapters,
        failure,
        raw_item_count: rows.len(),
    })
}

/// Parse a system_profiler memory string such as "8 GB" / "16384 MB" into
/// bytes. Unknown units or unparseable input degrade to None.
fn parse_memory_string(value: &str) -> Option<u64> {
    let mut tokens = value.split_whitespace();
    let amount = tokens.next().and_then(|token| token.parse::<u64>().ok())?;
    let unit = tokens.next()?.to_ascii_lowercase();
    let multiplier = match unit.as_str() {
        "b" => 1,
        "kb" => 1024,
        "mb" => 1024 * 1024,
        "gb" => 1024 * 1024 * 1024,
        "tb" => 1024_u64 * 1024 * 1024 * 1024,
        _ => return None,
    };
    amount.checked_mul(multiplier)
}

#[cfg(test)]
#[path = "../../tests/headless/macos_provider_gpu.rs"]
mod tests;
