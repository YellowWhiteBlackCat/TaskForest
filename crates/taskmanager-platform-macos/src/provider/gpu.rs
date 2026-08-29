//! macOS GPU inventory telemetry built on the safe `system_profiler` shell-out.
//!
//! `system_profiler SPDisplaysDataType -json` (run through the bounded command
//! runner) is the same safe source `MacHardwareInventoryProvider` uses for
//! model/chip facts. It publishes GPU identity (brand) and, for discrete
//! adapters, total VRAM. Live dynamic scalars (utilization, temperature, power,
//! frequency, fan) have NO safe macOS source — Metal/IOKit are unsafe and
//! sysinfo exposes no GPU accessor — so they stay honestly absent (None
//! options, no scalar observation) rather than fabricated zeros (ADR-019).

use std::time::Duration;

use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_core::{
    DeviceGeneration, DeviceState, FailureKind, GpuMetricField, GpuMetricProvenance, GpuMetrics,
    GpuScalarObservations, GpuTelemetryObservation, ProviderId, ScalarObservation,
};
use taskmanager_platform_contract::ProviderFailure;
use taskmanager_platform_provider::GpuTelemetryProvider;

use taskmanager_platform_portable::{BoundedCommandError, run_with_timeout};

const GPU_TELEMETRY_PROVIDER: ProviderId = ProviderId::borrowed("macos.telemetry.gpu");

fn unavailable_source(provider: ProviderId, failure: FailureKind) -> SourceStatus {
    SourceStatus {
        provider,
        outcome: SourceOutcome::Unavailable(failure),
        item_count: 0,
    }
}

/// GPU inventory telemetry: identity (brand) and total VRAM come from
/// `system_profiler SPDisplaysDataType -json`. Live dynamic scalars
/// (utilization, temperature, power, frequency, fan) have no safe macOS
/// source and stay honestly absent. A failed profiler invocation is typed as
/// unavailable; a successful inventory containing no adapters is an
/// authoritative empty result (ADR-019).
pub struct MacGpuTelemetryProvider;

impl GpuTelemetryProvider for MacGpuTelemetryProvider {
    fn refresh(&mut self, observed_at_ms: u64) -> Result<GpuTelemetryObservation, ProviderFailure> {
        Ok(gpu_observation_from_profiler(
            system_profiler_gpus(),
            observed_at_ms,
        ))
    }
}

fn gpu_observation_from_profiler(
    profiler_result: Result<Vec<MacGpuAdapter>, FailureKind>,
    observed_at_ms: u64,
) -> GpuTelemetryObservation {
    let mut adapters = match profiler_result {
        Ok(adapters) => adapters,
        Err(failure) => {
            return GpuTelemetryObservation::unavailable(
                failure,
                vec![unavailable_source(GPU_TELEMETRY_PROVIDER, failure)],
                Vec::new(),
                Default::default(),
            );
        }
    };
    if adapters.is_empty() {
        return GpuTelemetryObservation::current(
            Vec::new(),
            observed_at_ms,
            vec![SourceStatus {
                provider: GPU_TELEMETRY_PROVIDER,
                outcome: SourceOutcome::Empty,
                item_count: 0,
            }],
            Vec::new(),
            Default::default(),
        );
    }
    adapters.sort_by(|left, right| left.identity.cmp(&right.identity));
    let mut identities = std::collections::HashSet::<String>::new();
    let mut identity_partial = false;
    let mut gpus = Vec::with_capacity(adapters.len());
    for adapter in adapters {
        if !identities.insert(adapter.identity.clone()) {
            // Two rows with the same available hardware identity cannot be
            // separated safely by enumeration order.
            identity_partial = true;
            continue;
        }
        identity_partial |= !adapter.identity_is_authoritative;
        let mut provenance = vec![GpuMetricProvenance {
            field: GpuMetricField::Brand,
            provider: GPU_TELEMETRY_PROVIDER,
        }];
        let mut row = GpuMetrics::new(
            format!("macos:gpu:system-profiler:{}", adapter.identity),
            adapter.brand,
        );
        row.device_generation = DeviceGeneration::INITIAL;
        row.device_state = DeviceState::healthy(observed_at_ms);
        let mut observations = GpuScalarObservations::default();
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
    let gpu_count = gpus.len();
    let source = SourceStatus {
        provider: GPU_TELEMETRY_PROVIDER,
        outcome: if identity_partial {
            SourceOutcome::Partial(FailureKind::Unsupported)
        } else {
            SourceOutcome::Available
        },
        item_count: gpu_count,
    };
    GpuTelemetryObservation::current(
        gpus,
        observed_at_ms,
        vec![source],
        Vec::new(),
        Default::default(),
    )
}

/// One GPU adapter discovered by `system_profiler SPDisplaysDataType -json`.
struct MacGpuAdapter {
    identity: String,
    identity_is_authoritative: bool,
    brand: String,
    /// Total dedicated VRAM when system_profiler reports it (discrete GPUs).
    /// `None` on Apple Silicon unified-memory parts where system_profiler
    /// publishes no `spdisplays_vram` field.
    vram_total_bytes: Option<u64>,
}

/// Map one `SPDisplaysDataType` JSON row onto an adapter. Returns None for
/// rows that carry no GPU identity (e.g. display-only entries).
fn gpu_adapter_from_json(row: &serde_json::Value) -> Option<MacGpuAdapter> {
    let brand = row
        .get("_name")
        .or_else(|| row.get("sppci_model"))
        .and_then(serde_json::Value::as_str)
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())?;
    let vram_total_bytes = row
        .get("spdisplays_vram")
        .and_then(serde_json::Value::as_str)
        .and_then(parse_memory_string);
    let identity_parts = [
        "spdisplays_vendor-id",
        "spdisplays_device-id",
        "spdisplays_revision-id",
        "spdisplays_gmux-version",
    ]
    .into_iter()
    .filter_map(|key| {
        row.get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| format!("{key}={value}"))
    })
    .collect::<Vec<_>>();
    let identity_is_authoritative = !identity_parts.is_empty();
    let identity = if identity_is_authoritative {
        identity_parts.join(":")
    } else {
        format!("brand={}", brand.to_ascii_lowercase())
    };
    Some(MacGpuAdapter {
        identity,
        identity_is_authoritative,
        brand,
        vram_total_bytes,
    })
}

/// `system_profiler SPDisplaysDataType -json` → one entry per top-level GPU
/// adapter. Invocation failures remain distinct from a successful empty
/// inventory.
fn system_profiler_gpus() -> Result<Vec<MacGpuAdapter>, FailureKind> {
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

fn parse_system_profiler_gpus(output: &[u8]) -> Result<Vec<MacGpuAdapter>, FailureKind> {
    let root: serde_json::Value =
        serde_json::from_slice(output).map_err(|_| FailureKind::ProviderFault)?;
    let rows = root
        .get("SPDisplaysDataType")
        .and_then(serde_json::Value::as_array)
        .ok_or(FailureKind::ProviderFault)?;
    Ok(rows.iter().filter_map(gpu_adapter_from_json).collect())
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
