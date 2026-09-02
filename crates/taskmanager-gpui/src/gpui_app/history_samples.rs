//! Gap-aware presentation adapters for correlated system histories.
//!
//! This is deliberately a read-only boundary: collection and correlation stay
//! outside GPUI, while graphs receive finite values or explicit `NaN` gaps.

use taskmanager_telemetry_store::{
    CorrelatedMetricHistory, CorrelatedMetricSample, CorrelatedSystemTelemetryHistory,
    DeviceMetricHistory, DynamicTelemetryHistory,
};

use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;
use taskmanager_core::core::{DeviceGeneration, DeviceId, GpuEngineMetricPoint, GpuMetrics};

use crate::gpui_app::graph::GraphCacheHandle;

/// Which device-history family a cached sample vector was derived from.
///
/// The split-direction rate families (read/write, rx/tx) are keyed separately
/// from their summed lane: the directions come from distinct rings of the same
/// accepted events, so a summed-lane hit must never serve a direction window
/// even when device, generation, and watermark happen to agree.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum SampleFamily {
    StorageActivity,
    StorageTemperature,
    /// Summed read+write bytes/sec lane (decimal MB/s graph coordinates).
    StorageRate,
    /// Read-direction bytes/sec; see [`SampleFamily::StorageRate`].
    StorageReadRate,
    /// Write-direction bytes/sec; see [`SampleFamily::StorageRate`].
    StorageWriteRate,
    NetworkRate,
    /// Receive-direction bytes/sec — a split companion of
    /// [`SampleFamily::NetworkRate`] with its own per-direction gaps.
    NetworkRxRate,
    /// Transmit-direction bytes/sec; see [`SampleFamily::NetworkRxRate`].
    NetworkTxRate,
    GpuUsage,
    GpuEngine,
    BatteryCapacity,
    BatteryPower,
    FanRpm,
    FanTemperature,
}

/// Full input identity of one derived device sample vector. Because the ring
/// watermark (length + latest revision) is part of the key, a hit proves the
/// source history did not change since the vector was computed.
#[derive(Clone, PartialEq, Eq, Hash)]
struct DeviceSampleKey {
    family: SampleFamily,
    device: String,
    /// Series discriminator within one family+device (e.g. the GPU engine
    /// name). Empty for families that expose exactly one series per device.
    variant: String,
    generation: u64,
    len: usize,
    last_revision: Option<u64>,
    /// Identity of the underlying ring storage: distinct histories that
    /// happen to agree on the watermark can never serve each other's cached
    /// vector (two test stores with the same device id, say).
    ring: usize,
}

/// Per-device graph sample vectors owned by one GPUI window's graph cache.
///
/// These are pure derived values keyed by the accepted ring watermark, but
/// their allocation lifetime is still renderer state. Keeping the map inside
/// `RootView` prevents one window from retaining another window's generations.
#[derive(Default)]
pub(crate) struct DeviceSampleCache {
    entries: HashMap<DeviceSampleKey, Rc<[f32]>>,
}

const DEVICE_SAMPLE_CACHE_BOUND: usize = 512;

fn cached_device_samples<T>(
    graph_cache: &GraphCacheHandle,
    family: SampleFamily,
    history: Option<&DeviceMetricHistory<T>>,
    device: &str,
    variant: &str,
    generation: DeviceGeneration,
    compute: impl FnOnce() -> Vec<f32>,
) -> Rc<[f32]> {
    let (len, last_revision) = history.map_or((0, None), DeviceMetricHistory::watermark);
    let key = DeviceSampleKey {
        family,
        device: device.to_owned(),
        variant: variant.to_owned(),
        generation: generation.get(),
        len,
        last_revision,
        ring: history.map_or(0, DeviceMetricHistory::ring_id),
    };
    graph_cache.borrow_mut().with_device_samples(|cache| {
        if cache.entries.len() >= DEVICE_SAMPLE_CACHE_BOUND {
            cache.entries.clear();
        }
        cache
            .entries
            .entry(key)
            // `Rc::from(Vec)` moves the computed buffer; a cache hit hands the
            // SAME unsized `Rc` to every graph on UI-only frames.
            .or_insert_with(|| Rc::from(compute()))
            .clone()
    })
}

const U16_RADIX: f64 = 65_536.0;
const DECIMAL_BYTES_PER_MEGABYTE: f64 = 1_000_000.0;

pub(crate) fn f32_history_samples(history: CorrelatedMetricHistory<f32>) -> Vec<f32> {
    f32_samples(&history.samples())
}

pub(crate) fn storage_activity_samples(
    graph_cache: &GraphCacheHandle,
    history: &CorrelatedSystemTelemetryHistory,
    device_id: &str,
    generation: DeviceGeneration,
) -> Rc<[f32]> {
    let device = DeviceId::new(device_id.to_owned());
    let handle = history.storage_activity(&device);
    cached_device_samples(
        graph_cache,
        SampleFamily::StorageActivity,
        handle.as_ref(),
        device_id,
        "",
        generation,
        || {
            matching_device_samples(handle.clone(), generation)
                .map_or_else(Vec::new, |samples| f32_samples(&samples))
        },
    )
}

pub(crate) fn storage_temperature_samples(
    graph_cache: &GraphCacheHandle,
    history: &CorrelatedSystemTelemetryHistory,
    device_id: &str,
    generation: DeviceGeneration,
) -> Rc<[f32]> {
    let device = DeviceId::new(device_id.to_owned());
    let handle = history.storage_temperature_c(&device);
    cached_device_samples(
        graph_cache,
        SampleFamily::StorageTemperature,
        handle.as_ref(),
        device_id,
        "",
        generation,
        || {
            matching_device_samples(handle.clone(), generation)
                .map_or_else(Vec::new, |samples| f32_samples(&samples))
        },
    )
}

pub(crate) fn network_rate_samples(
    graph_cache: &GraphCacheHandle,
    history: &CorrelatedSystemTelemetryHistory,
    device_id: &str,
    generation: DeviceGeneration,
) -> Rc<[f32]> {
    let device = DeviceId::new(device_id.to_owned());
    let handle = history.network_rate(&device);
    cached_device_samples(
        graph_cache,
        SampleFamily::NetworkRate,
        handle.as_ref(),
        device_id,
        "",
        generation,
        || {
            matching_device_samples(handle.clone(), generation).map_or_else(Vec::new, |samples| {
                u64_samples(&samples, DECIMAL_BYTES_PER_MEGABYTE)
            })
        },
    )
}

/// Shared shape for the per-device throughput families (u64 bytes/sec rings
/// projected to decimal MB/s graph coordinates, matching the summed network
/// lane's historical coordinate space): one cached window per
/// family+device+generation+watermark. `ring` resolves the family's handle on
/// the correlated history; a missing ring yields the empty collecting window.
fn device_rate_samples(
    graph_cache: &GraphCacheHandle,
    family: SampleFamily,
    device_id: &str,
    generation: DeviceGeneration,
    ring: impl FnOnce(&DeviceId) -> Option<DeviceMetricHistory<u64>>,
) -> Rc<[f32]> {
    let device = DeviceId::new(device_id.to_owned());
    let handle = ring(&device);
    cached_device_samples(
        graph_cache,
        family,
        handle.as_ref(),
        device_id,
        "",
        generation,
        || {
            matching_device_samples(handle.clone(), generation).map_or_else(Vec::new, |samples| {
                u64_samples(&samples, DECIMAL_BYTES_PER_MEGABYTE)
            })
        },
    )
}

/// The disk's summed read+write throughput window (decimal MB/s). The disk
/// page's aggregate summary and first-frame state consume this lane; the main
/// graph strokes the split-direction companions below.
pub(crate) fn storage_rate_samples(
    graph_cache: &GraphCacheHandle,
    history: &CorrelatedSystemTelemetryHistory,
    device_id: &str,
    generation: DeviceGeneration,
) -> Rc<[f32]> {
    device_rate_samples(
        graph_cache,
        SampleFamily::StorageRate,
        device_id,
        generation,
        |device| history.storage_rate(device),
    )
}

/// The disk's read-direction throughput window (decimal MB/s) with its OWN
/// per-direction gaps — a missing read observation is `NaN`, never a
/// fabricated zero and never the write lane's value.
pub(crate) fn storage_read_rate_samples(
    graph_cache: &GraphCacheHandle,
    history: &CorrelatedSystemTelemetryHistory,
    device_id: &str,
    generation: DeviceGeneration,
) -> Rc<[f32]> {
    device_rate_samples(
        graph_cache,
        SampleFamily::StorageReadRate,
        device_id,
        generation,
        |device| history.storage_read_rate(device),
    )
}

/// The disk's write-direction throughput window; see
/// [`storage_read_rate_samples`].
pub(crate) fn storage_write_rate_samples(
    graph_cache: &GraphCacheHandle,
    history: &CorrelatedSystemTelemetryHistory,
    device_id: &str,
    generation: DeviceGeneration,
) -> Rc<[f32]> {
    device_rate_samples(
        graph_cache,
        SampleFamily::StorageWriteRate,
        device_id,
        generation,
        |device| history.storage_write_rate(device),
    )
}

/// The adapter's receive-direction throughput window (decimal MB/s) with its
/// OWN per-direction gaps; the summed `network_rate_samples` lane stays the
/// aggregate summary's authority.
pub(crate) fn network_rx_rate_samples(
    graph_cache: &GraphCacheHandle,
    history: &CorrelatedSystemTelemetryHistory,
    device_id: &str,
    generation: DeviceGeneration,
) -> Rc<[f32]> {
    device_rate_samples(
        graph_cache,
        SampleFamily::NetworkRxRate,
        device_id,
        generation,
        |device| history.network_rx_rate(device),
    )
}

/// The adapter's transmit-direction throughput window; see
/// [`network_rx_rate_samples`].
pub(crate) fn network_tx_rate_samples(
    graph_cache: &GraphCacheHandle,
    history: &CorrelatedSystemTelemetryHistory,
    device_id: &str,
    generation: DeviceGeneration,
) -> Rc<[f32]> {
    device_rate_samples(
        graph_cache,
        SampleFamily::NetworkTxRate,
        device_id,
        generation,
        |device| history.network_tx_rate(device),
    )
}

pub(crate) fn gpu_usage_samples(
    graph_cache: &GraphCacheHandle,
    history: &CorrelatedSystemTelemetryHistory,
    device_id: &str,
    generation: DeviceGeneration,
) -> Rc<[f32]> {
    let device = DeviceId::new(device_id.to_owned());
    let handle = history.gpu_usage(&device);
    cached_device_samples(
        graph_cache,
        SampleFamily::GpuUsage,
        handle.as_ref(),
        device_id,
        "",
        generation,
        || {
            matching_device_samples(handle.clone(), generation)
                .map_or_else(Vec::new, |samples| f32_samples(&samples))
        },
    )
}

/// Return the stable provider-neutral engine labels available in the current
/// point or in this device generation's typed history. Historical labels stay
/// visible through a transient refresh gap, while a generation change drops
/// them with the rest of the device history.
pub(crate) fn gpu_engine_series_names(
    history: &CorrelatedSystemTelemetryHistory,
    metrics: &GpuMetrics,
) -> Vec<String> {
    let mut names = BTreeSet::new();
    if let Some(point) = GpuEngineMetricPoint::from_metrics(metrics) {
        names.extend(point.engines.into_iter().map(|engine| engine.name));
    }
    let device_id = DeviceId::new(metrics.device_id.clone());
    if let Some(samples) = matching_device_samples(
        history.gpu_engine_metrics(&device_id),
        metrics.device_generation,
    ) {
        for sample in samples {
            if let Some(point) = sample.value {
                names.extend(point.engines.into_iter().map(|engine| engine.name));
            }
        }
    }
    names.into_iter().collect()
}

/// Project one named engine's generation-scoped history into graph samples.
/// A missing engine in an otherwise valid point is a gap for that engine only;
/// it must not reuse a neighboring engine's value.
pub(crate) fn gpu_engine_samples(
    graph_cache: &GraphCacheHandle,
    history: &CorrelatedSystemTelemetryHistory,
    device_id: &str,
    generation: DeviceGeneration,
    engine_name: &str,
) -> Rc<[f32]> {
    let device_id = DeviceId::new(device_id.to_owned());
    let handle = history.gpu_engine_metrics(&device_id);
    cached_device_samples(
        graph_cache,
        SampleFamily::GpuEngine,
        handle.as_ref(),
        device_id.as_str(),
        engine_name,
        generation,
        || {
            matching_device_samples(handle.clone(), generation).map_or_else(Vec::new, |samples| {
                samples
                    .iter()
                    .map(
                        |sample| match (sample.measured_at_ms, sample.value.as_ref()) {
                            (Some(_), Some(point)) => point
                                .engines
                                .iter()
                                .find(|engine| engine.name == engine_name)
                                .map_or(f32::NAN, |engine| engine.utilization_pct),
                            _ => f32::NAN,
                        },
                    )
                    .collect()
            })
        },
    )
}

pub(crate) fn battery_capacity_samples(
    graph_cache: &GraphCacheHandle,
    history: &DynamicTelemetryHistory,
    device_id: &str,
    generation: DeviceGeneration,
) -> Rc<[f32]> {
    let device = DeviceId::new(device_id.to_owned());
    let handle = history.battery_capacity_pct(&device);
    cached_device_samples(
        graph_cache,
        SampleFamily::BatteryCapacity,
        handle.as_ref(),
        device_id,
        "",
        generation,
        || {
            matching_device_samples(handle.clone(), generation)
                .map_or_else(Vec::new, |samples| f32_samples(&samples))
        },
    )
}

pub(crate) fn battery_power_samples(
    graph_cache: &GraphCacheHandle,
    history: &DynamicTelemetryHistory,
    device_id: &str,
    generation: DeviceGeneration,
) -> Rc<[f32]> {
    let device = DeviceId::new(device_id.to_owned());
    let handle = history.battery_power_w(&device);
    cached_device_samples(
        graph_cache,
        SampleFamily::BatteryPower,
        handle.as_ref(),
        device_id,
        "",
        generation,
        || {
            matching_device_samples(handle.clone(), generation)
                .map_or_else(Vec::new, |samples| f32_samples(&samples))
        },
    )
}

pub(crate) fn fan_rpm_samples(
    graph_cache: &GraphCacheHandle,
    history: &DynamicTelemetryHistory,
    channel_id: &str,
    generation: DeviceGeneration,
) -> Rc<[f32]> {
    let channel = DeviceId::new(channel_id.to_owned());
    let handle = history.fan_rpm(&channel);
    cached_device_samples(
        graph_cache,
        SampleFamily::FanRpm,
        handle.as_ref(),
        channel_id,
        "",
        generation,
        || {
            matching_device_samples(handle.clone(), generation)
                .map_or_else(Vec::new, |samples| f32_samples(&samples))
        },
    )
}

pub(crate) fn fan_temperature_samples(
    graph_cache: &GraphCacheHandle,
    history: &DynamicTelemetryHistory,
    channel_id: &str,
    generation: DeviceGeneration,
) -> Rc<[f32]> {
    let channel = DeviceId::new(channel_id.to_owned());
    let handle = history.fan_temperature_c(&channel);
    cached_device_samples(
        graph_cache,
        SampleFamily::FanTemperature,
        handle.as_ref(),
        channel_id,
        "",
        generation,
        || {
            matching_device_samples(handle.clone(), generation)
                .map_or_else(Vec::new, |samples| f32_samples(&samples))
        },
    )
}

/// All graph lanes the Disk page consumes for one device. Keeping the lookup
/// set together prevents a page caller from silently mixing a cached lane
/// with a different generation or direction.
pub(crate) struct DiskGraphSamples {
    pub read: Rc<[f32]>,
    pub write: Rc<[f32]>,
    pub aggregate: Rc<[f32]>,
    pub temperature: Rc<[f32]>,
    pub activity: Rc<[f32]>,
}

pub(crate) fn disk_graph_samples(
    graph_cache: &GraphCacheHandle,
    history: &CorrelatedSystemTelemetryHistory,
    device_id: &str,
    generation: DeviceGeneration,
) -> DiskGraphSamples {
    DiskGraphSamples {
        read: storage_read_rate_samples(graph_cache, history, device_id, generation),
        write: storage_write_rate_samples(graph_cache, history, device_id, generation),
        aggregate: storage_rate_samples(graph_cache, history, device_id, generation),
        temperature: storage_temperature_samples(graph_cache, history, device_id, generation),
        activity: storage_activity_samples(graph_cache, history, device_id, generation),
    }
}

/// All graph lanes the Network page consumes for one adapter; receive and
/// transmit stay distinct from their aggregate summary lane.
pub(crate) struct NetworkGraphSamples {
    pub receive: Rc<[f32]>,
    pub transmit: Rc<[f32]>,
    pub aggregate: Rc<[f32]>,
}

pub(crate) fn network_graph_samples(
    graph_cache: &GraphCacheHandle,
    history: &CorrelatedSystemTelemetryHistory,
    device_id: &str,
    generation: DeviceGeneration,
) -> NetworkGraphSamples {
    NetworkGraphSamples {
        receive: network_rx_rate_samples(graph_cache, history, device_id, generation),
        transmit: network_tx_rate_samples(graph_cache, history, device_id, generation),
        aggregate: network_rate_samples(graph_cache, history, device_id, generation),
    }
}

fn matching_device_samples<T: Clone>(
    history: Option<DeviceMetricHistory<T>>,
    generation: DeviceGeneration,
) -> Option<Vec<CorrelatedMetricSample<T>>> {
    let generation = generation.get();
    if generation == 0 {
        return None;
    }
    let history = history?;
    generation_scoped_samples(history.generation(), generation, history.samples())
}

fn generation_scoped_samples<T>(
    history_generation: u64,
    expected_generation: u64,
    samples: Vec<CorrelatedMetricSample<T>>,
) -> Option<Vec<CorrelatedMetricSample<T>>> {
    (history_generation == expected_generation).then_some(samples)
}

fn f32_samples(samples: &[CorrelatedMetricSample<f32>]) -> Vec<f32> {
    samples
        .iter()
        .map(|sample| match (sample.measured_at_ms, sample.value) {
            (Some(_), Some(value)) if value.is_finite() => value,
            _ => f32::NAN,
        })
        .collect()
}

fn u64_samples(samples: &[CorrelatedMetricSample<u64>], divisor: f64) -> Vec<f32> {
    samples
        .iter()
        .map(|sample| match (sample.measured_at_ms, sample.value) {
            (Some(_), Some(value)) => bounded_graph_f32(u64_as_f64(value) / divisor),
            _ => f32::NAN,
        })
        .collect()
}

/// Convert the full `u64` domain without an unchecked integer-to-float cast.
///
/// Four base-2^16 digits reconstruct the value in `f64`. Graph storage is
/// necessarily approximate above 24 significant bits, so the final narrowing
/// is explicitly range-bounded before conversion. Every `u64` is well within
/// the finite `f32` exponent range.
fn u64_as_f64(value: u64) -> f64 {
    value
        .to_be_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|bytes| f64::from(u16::from_be_bytes(*bytes)))
        .fold(0.0, |accumulator, word| {
            accumulator.mul_add(U16_RADIX, word)
        })
}

fn bounded_graph_f32(value: f64) -> f32 {
    let bounded = value.clamp(f64::from(f32::MIN), f64::from(f32::MAX));
    bounded as f32
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_history_samples_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_history_samples_device_cache_tests.rs"]
mod device_cache_tests;
