//! Gap-aware presentation adapters for correlated system histories.
//!
//! This is deliberately a read-only boundary: collection and correlation stay
//! outside GPUI, while graphs receive finite values or explicit `NaN` gaps.

use taskmanager_telemetry_store::{
    CorrelatedMetricHistory, CorrelatedMetricSample, CorrelatedSystemTelemetryHistory,
    DeviceMetricHistory, DynamicTelemetryHistory,
};

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;
use taskmanager_core::core::{DeviceGeneration, DeviceId, GpuEngineMetricPoint, GpuMetrics};

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

// Per-device graph sample vectors, cached by content watermark. These are
// PURE derived data (a function of the key alone), so — unlike a preference
// thread_local — sharing them across windows cannot leak one window's state
// into another: the same key always produces the same vector. Graph
// animations and hover re-render these projections every frame while the
// underlying rings tick at ~1-2 Hz; the cache collapses that to an `Rc`
// clone. Bounded by clearing wholesale past a generous device×family bound.
thread_local! {
    static DEVICE_SAMPLE_CACHE: RefCell<HashMap<DeviceSampleKey, Rc<[f32]>>> =
        RefCell::new(HashMap::new());
}

const DEVICE_SAMPLE_CACHE_BOUND: usize = 512;

fn cached_device_samples<T>(
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
    DEVICE_SAMPLE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= DEVICE_SAMPLE_CACHE_BOUND {
            cache.clear();
        }
        cache
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
    history: &CorrelatedSystemTelemetryHistory,
    device_id: &str,
    generation: DeviceGeneration,
) -> Rc<[f32]> {
    let device = DeviceId::new(device_id.to_owned());
    let handle = history.storage_activity(&device);
    cached_device_samples(
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
    history: &CorrelatedSystemTelemetryHistory,
    device_id: &str,
    generation: DeviceGeneration,
) -> Rc<[f32]> {
    let device = DeviceId::new(device_id.to_owned());
    let handle = history.storage_temperature_c(&device);
    cached_device_samples(
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
    history: &CorrelatedSystemTelemetryHistory,
    device_id: &str,
    generation: DeviceGeneration,
) -> Rc<[f32]> {
    let device = DeviceId::new(device_id.to_owned());
    let handle = history.network_rate(&device);
    cached_device_samples(
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
    family: SampleFamily,
    device_id: &str,
    generation: DeviceGeneration,
    ring: impl FnOnce(&DeviceId) -> Option<DeviceMetricHistory<u64>>,
) -> Rc<[f32]> {
    let device = DeviceId::new(device_id.to_owned());
    let handle = ring(&device);
    cached_device_samples(family, handle.as_ref(), device_id, "", generation, || {
        matching_device_samples(handle.clone(), generation).map_or_else(Vec::new, |samples| {
            u64_samples(&samples, DECIMAL_BYTES_PER_MEGABYTE)
        })
    })
}

/// The disk's summed read+write throughput window (decimal MB/s). The disk
/// page's aggregate summary and first-frame state consume this lane; the main
/// graph strokes the split-direction companions below.
pub(crate) fn storage_rate_samples(
    history: &CorrelatedSystemTelemetryHistory,
    device_id: &str,
    generation: DeviceGeneration,
) -> Rc<[f32]> {
    device_rate_samples(SampleFamily::StorageRate, device_id, generation, |device| {
        history.storage_rate(device)
    })
}

/// The disk's read-direction throughput window (decimal MB/s) with its OWN
/// per-direction gaps — a missing read observation is `NaN`, never a
/// fabricated zero and never the write lane's value.
pub(crate) fn storage_read_rate_samples(
    history: &CorrelatedSystemTelemetryHistory,
    device_id: &str,
    generation: DeviceGeneration,
) -> Rc<[f32]> {
    device_rate_samples(
        SampleFamily::StorageReadRate,
        device_id,
        generation,
        |device| history.storage_read_rate(device),
    )
}

/// The disk's write-direction throughput window; see
/// [`storage_read_rate_samples`].
pub(crate) fn storage_write_rate_samples(
    history: &CorrelatedSystemTelemetryHistory,
    device_id: &str,
    generation: DeviceGeneration,
) -> Rc<[f32]> {
    device_rate_samples(
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
    history: &CorrelatedSystemTelemetryHistory,
    device_id: &str,
    generation: DeviceGeneration,
) -> Rc<[f32]> {
    device_rate_samples(
        SampleFamily::NetworkRxRate,
        device_id,
        generation,
        |device| history.network_rx_rate(device),
    )
}

/// The adapter's transmit-direction throughput window; see
/// [`network_rx_rate_samples`].
pub(crate) fn network_tx_rate_samples(
    history: &CorrelatedSystemTelemetryHistory,
    device_id: &str,
    generation: DeviceGeneration,
) -> Rc<[f32]> {
    device_rate_samples(
        SampleFamily::NetworkTxRate,
        device_id,
        generation,
        |device| history.network_tx_rate(device),
    )
}

pub(crate) fn gpu_usage_samples(
    history: &CorrelatedSystemTelemetryHistory,
    device_id: &str,
    generation: DeviceGeneration,
) -> Rc<[f32]> {
    let device = DeviceId::new(device_id.to_owned());
    let handle = history.gpu_usage(&device);
    cached_device_samples(
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
    history: &CorrelatedSystemTelemetryHistory,
    device_id: &str,
    generation: DeviceGeneration,
    engine_name: &str,
) -> Rc<[f32]> {
    let device_id = DeviceId::new(device_id.to_owned());
    let handle = history.gpu_engine_metrics(&device_id);
    cached_device_samples(
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
    history: &DynamicTelemetryHistory,
    device_id: &str,
    generation: DeviceGeneration,
) -> Rc<[f32]> {
    let device = DeviceId::new(device_id.to_owned());
    let handle = history.battery_capacity_pct(&device);
    cached_device_samples(
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
    history: &DynamicTelemetryHistory,
    device_id: &str,
    generation: DeviceGeneration,
) -> Rc<[f32]> {
    let device = DeviceId::new(device_id.to_owned());
    let handle = history.battery_power_w(&device);
    cached_device_samples(
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
    history: &DynamicTelemetryHistory,
    channel_id: &str,
    generation: DeviceGeneration,
) -> Rc<[f32]> {
    let channel = DeviceId::new(channel_id.to_owned());
    let handle = history.fan_rpm(&channel);
    cached_device_samples(
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
    history: &DynamicTelemetryHistory,
    channel_id: &str,
    generation: DeviceGeneration,
) -> Rc<[f32]> {
    let channel = DeviceId::new(channel_id.to_owned());
    let handle = history.fan_temperature_c(&channel);
    cached_device_samples(
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
