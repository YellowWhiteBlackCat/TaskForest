//! Per-core and aggregate utilization cache consumed by the fixed CPU page.

use std::rc::Rc;

use taskmanager_telemetry_store::TelemetryStore;

use crate::gpui_app::history_samples::f32_history_samples;

/// Generation-keyed cache of the per-core utilization history projection.
///
/// CPU core history changes only when the platform batch accepts a CPU-domain
/// system outcome, so every other render (hover, keyboard, resize) reuses the
/// previous projection instead of re-cloning each per-core history out of the
/// telemetry store and re-projecting it to `f32` samples. `bump()` is called
/// from the batch-apply path; the projection is rebuilt lazily on the next
/// render that sees an advanced generation — the same keying as
/// `RootView::processes_projection`. The per-metric auto-scale ceilings are
/// computed in the same rebuild, so a hover frame no longer rescans every
/// core × sample.
pub struct CpuHistoryCache {
    /// Generation the cached projection was built at; `None` until the first
    /// render, so an empty cache always builds.
    built_at_generation: Option<u64>,
    /// Monotonic generation. Advanced once per accepted CPU-domain outcome.
    generation: u64,
    /// Per-logical-core sample projections (oldest..newest, `NaN` = gap),
    /// shared with the fixed per-core utilization canvases via `Rc` clone.
    per_core_usage: Vec<Rc<[f32]>>,
    /// Aggregate utilization waveform, rebuilt in the same generation-keyed
    /// pass so the CPU headline graph and sidebar share one projection.
    aggregate_usage: Rc<[f32]>,
}

/// Borrowed view over the cached per-core series families.
pub(crate) struct PerCoreSeries<'a> {
    pub usage: &'a [Rc<[f32]>],
}

/// Owned headline utilization series, sharing the cache's generation rebuild.
pub(crate) struct CpuAggregateSeries {
    pub usage: Rc<[f32]>,
}

impl CpuHistoryCache {
    pub(crate) fn new() -> Self {
        Self {
            built_at_generation: None,
            generation: 0,
            per_core_usage: Vec::new(),
            aggregate_usage: Rc::from([]),
        }
    }

    /// The store accepted a CPU-domain outcome; the next render must rebuild
    /// the projection.
    pub(crate) fn bump(&mut self) {
        self.generation += 1;
    }

    /// Rebuild every cached family when the generation advanced since the last
    /// build (shared by the per-core grid and the headline graphs).
    fn rebuild_if_stale(&mut self, telemetry: &TelemetryStore) {
        if self.built_at_generation == Some(self.generation) {
            return;
        }
        self.built_at_generation = Some(self.generation);
        self.per_core_usage = telemetry
            .system_history
            .cpu_core_usage()
            .into_iter()
            .map(|history| Rc::from(f32_history_samples(history)))
            .collect();
        self.aggregate_usage = Rc::from(f32_history_samples(telemetry.system_history.cpu_usage()));
    }

    /// Render entry: rebuild the per-core projections only when the
    /// generation advanced since the last build, then return them as a borrow.
    pub(super) fn refresh(&mut self, telemetry: &TelemetryStore) -> PerCoreSeries<'_> {
        self.rebuild_if_stale(telemetry);
        PerCoreSeries {
            usage: &self.per_core_usage,
        }
    }

    /// Render entry for the aggregate graph (see [`Self::refresh`]).
    pub(crate) fn aggregate(&mut self, telemetry: &TelemetryStore) -> CpuAggregateSeries {
        self.rebuild_if_stale(telemetry);
        CpuAggregateSeries {
            usage: Rc::clone(&self.aggregate_usage),
        }
    }
}

/// The readout pinned to one per-core cell. The plotted metric stays first so
/// the label cannot disagree with the curve, while an available per-core
/// temperature remains visible on every overlay instead of disappearing when
/// the utilization or frequency curve is selected.
pub(crate) fn per_core_cell_label(
    usage_pct: Option<f32>,
    temperature_c: Option<f32>,
    frequency_mhz: Option<u64>,
) -> String {
    let usage = usage_pct
        .filter(|value| value.is_finite())
        .map(|value| format!("{value:.0} %"));
    let frequency =
        frequency_mhz.map(|value| crate::gpui_app::formatting::optional_ghz(Some(value)));
    let temperature = temperature_c
        .filter(|value| value.is_finite())
        .map(taskmanager_shell::presentation::temperature_c);
    let mut parts = Vec::with_capacity(3);
    parts.extend(usage);
    parts.extend(frequency);
    parts.extend(temperature);
    if parts.is_empty() {
        crate::gpui_app::formatting::missing_value()
    } else {
        parts.join(" · ")
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_cpu_view_per_core_projection_tests.rs"]
mod projection_tests;

/// Headline-cache behavior: the aggregate waveform families are rebuilt only
/// when the CPU-domain generation advances, and consecutive UI-only frames
/// reuse the same `Rc` (the property that keeps hover/resize frames from
/// re-extracting the correlated histories).
#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_cpu_view_per_core_aggregate_cache_tests.rs"]
mod aggregate_cache_tests;
