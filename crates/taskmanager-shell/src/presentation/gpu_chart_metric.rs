//! Shared GPU chart-metric selection model and availability reconcile
//! (ADR-034 stage 1 typed contract; stage 2 wires the three frontends onto
//! it through the per-tick viewed-GPU folds defined here).
//!
//! The vocabulary is the telemetry-store GPU series families: one
//! [`GpuChartMetric`] per chartable `GpuMetricPoint` field, in the ADR-034
//! listing order, with no second enum semantics (ADR-034 decision 1:
//! “序列词汇与 telemetry-store 既有 GPU 序列族一一对应，不新造第二套枚举语义”).
//!
//! The selection is session-scoped, per-window state defined once in the
//! shell: never persisted, never part of `Config` (ADR-034 decision 1). The
//! default is Utilization. Availability gates the vocabulary through the
//! typed [`GpuChartMetricAvailability`]: an unavailable series is rejected as
//! a selection target and projects as explicitly unavailable — never removed
//! from the vocabulary, never fabricated as a measured zero, and never hiding
//! the remaining series (ADR-034 decision 1 and 验收约束: “不可用序列保持显式
//! 不可用投影（dash/gap）；一个序列的失败不影响其他序列、其他设备或其他字段”).
//! A device-generation change resets the selection to the default (ADR-034
//! stage-1 gate: “设备 generation 变化后的回退”).
//!
//! ADR-034 names the default only as the generation-reset target; it does
//! not name the fallback target for a selection that becomes unavailable
//! under the same generation. The conservative choice here: the same default,
//! and when even the default is unavailable the selection stays on the
//! default and projects [`GpuChartMetricChoiceState::SelectedUnavailable`] —
//! the explicit dash/gap degradation of the 验收约束 — instead of silently
//! swapping to a different series.
//!
//! Everything here is pure: no I/O, threads, or time (ADR-034 阶段 1:
//! “此阶段不触碰任何 renderer”; stage 2 wires the frontends onto this
//! contract, stage 3 adds Bevy).

use taskmanager_core::core::metrics::GpuMetrics;
use taskmanager_telemetry_store::GpuMetricPoint;
use taskmanager_telemetry_store::live_graph::LiveGraphHistory;

/// One chartable GPU series family (ADR-034 decision 1).
///
/// Variants correspond one-to-one with the `GpuMetricPoint` fields the
/// telemetry-store GPU history retains; [`Self::ALL`] is the fixed
/// presentation and cycle order — the ADR-034 vocabulary listing order
/// (Util/Power/Temp/Freq/Memory/DedicatedMemory/SharedMemory/IdleResidency).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GpuChartMetric {
    /// Aggregate utilization percent.
    Utilization,
    /// Board power draw in watts.
    Power,
    /// Die temperature in °C.
    Temperature,
    /// Core clock in MHz.
    Frequency,
    /// Overall memory as a used/total percent.
    Memory,
    /// Dedicated on-card VRAM as a used/total percent.
    DedicatedMemory,
    /// Shared/system aperture memory as a used/total percent.
    SharedMemory,
    /// Idle residency percent.
    IdleResidency,
}

impl GpuChartMetric {
    /// Fixed vocabulary order shared by the selector projection and the
    /// cycle — ADR-034 stage 1: “序列循环（cycle 顺序固定）”.
    pub const ALL: [Self; 8] = [
        Self::Utilization,
        Self::Power,
        Self::Temperature,
        Self::Frequency,
        Self::Memory,
        Self::DedicatedMemory,
        Self::SharedMemory,
        Self::IdleResidency,
    ];

    /// The ADR-034 default selection.
    pub const DEFAULT: Self = Self::Utilization;

    /// The localized series label key every frontend renders for this family
    /// (the retired selector's key spelling, restored once here so the three
    /// stage-2 frontends share one pill/axis/title wording).
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Utilization => "gpu.graph_utilization",
            Self::Power => "gpu.graph_power",
            Self::Temperature => "gpu.graph_temperature",
            Self::Frequency => "gpu.graph_frequency",
            Self::Memory => "gpu.graph_memory",
            Self::DedicatedMemory => "gpu.graph_dedicated_memory",
            Self::SharedMemory => "gpu.graph_shared_memory",
            Self::IdleResidency => "gpu.graph_idle_residency",
        }
    }

    /// Stable identity stem for renderer focus/element ids (one spelling for
    /// the Iced focus ids, the GPUI element ids, and test selectors).
    pub const fn id_stem(self) -> &'static str {
        match self {
            Self::Utilization => "utilization",
            Self::Power => "power",
            Self::Temperature => "temperature",
            Self::Frequency => "frequency",
            Self::Memory => "memory",
            Self::DedicatedMemory => "dedicated-memory",
            Self::SharedMemory => "shared-memory",
            Self::IdleResidency => "idle-residency",
        }
    }

    /// The chart-unit family of this series — shared so every frontend's
    /// axis labels, badges, and summaries agree on what one series measures
    /// (memory families are used/total percentages, never raw bytes).
    pub const fn unit(self) -> GpuChartMetricUnit {
        match self {
            Self::Utilization
            | Self::Memory
            | Self::DedicatedMemory
            | Self::SharedMemory
            | Self::IdleResidency => GpuChartMetricUnit::Percent,
            Self::Power => GpuChartMetricUnit::Watts,
            Self::Temperature => GpuChartMetricUnit::Celsius,
            Self::Frequency => GpuChartMetricUnit::Megahertz,
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::Utilization => 0,
            Self::Power => 1,
            Self::Temperature => 2,
            Self::Frequency => 3,
            Self::Memory => 4,
            Self::DedicatedMemory => 5,
            Self::SharedMemory => 6,
            Self::IdleResidency => 7,
        }
    }

    /// The single scalar a chart of this series renders from one typed GPU
    /// metric point: source units for scalar families, a used/total percent
    /// for the memory families. Memory needs both sides and a non-zero
    /// denominator — a missing or zero capacity stays unavailable, never a
    /// believable percent. The availability gate below is derived from the
    /// same fold, so the selector and the chart can never disagree about
    /// what one series carries.
    #[must_use]
    pub fn value(self, point: &GpuMetricPoint) -> Option<f32> {
        match self {
            Self::Utilization => finite(point.utilization_pct),
            Self::Power => finite(point.power_w),
            Self::Temperature => finite(point.temperature_c),
            Self::Frequency => point.frequency_mhz.map(|mhz| mhz as f32),
            Self::Memory => bytes_percentage(point.memory_used_bytes, point.memory_total_bytes),
            Self::DedicatedMemory => bytes_percentage(
                point.dedicated_memory_used_bytes,
                point.dedicated_memory_total_bytes,
            ),
            Self::SharedMemory => bytes_percentage(
                point.shared_memory_used_bytes,
                point.shared_memory_total_bytes,
            ),
            Self::IdleResidency => finite(point.idle_residency_pct),
        }
    }
}

/// Typed per-series availability for one GPU device, derived from its latest
/// typed metric point (ADR-034 decision 1: availability gates the selection
/// vocabulary). Each family is gated independently, so one failed sensor
/// carries no information about the other series, other devices, or other
/// fields (ADR-034 验收约束).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GpuChartMetricAvailability {
    slots: [bool; 8],
}

impl GpuChartMetricAvailability {
    /// Every series unavailable — the state before any typed GPU point
    /// exists. A selection reconciled against this stays on the default and
    /// projects explicit unavailability; it never gains a fabricated series.
    #[must_use]
    pub const fn unavailable() -> Self {
        Self { slots: [false; 8] }
    }

    /// Derive availability from the latest typed GPU metric point: a series
    /// family is available exactly when the point carries its chartable
    /// scalar ([`GpuChartMetric::value`]).
    #[must_use]
    pub fn from_gpu_metric_point(point: &GpuMetricPoint) -> Self {
        Self {
            slots: GpuChartMetric::ALL.map(|metric| metric.value(point).is_some()),
        }
    }

    /// Whether one series family passes the availability gate.
    #[must_use]
    pub const fn is_available(&self, metric: GpuChartMetric) -> bool {
        self.slots[metric.index()]
    }

    /// The availability fold for the device a window is currently viewing
    /// (ADR-034 stage 2): derived from the viewed GPU row's latest typed
    /// point, or honestly all-unavailable when no GPU is viewed. The
    /// shell-state folds below route every frontend through this single
    /// derivation — renderers never re-derive per-series gates.
    #[must_use]
    pub fn for_viewed_gpu(gpu: Option<&GpuMetrics>) -> Self {
        gpu.map_or_else(Self::unavailable, |gpu| {
            Self::from_gpu_metric_point(&GpuMetricPoint::from_metrics(gpu))
        })
    }
}

/// The copyable gate inputs for one tick of the shared chart-metric fold
/// (ADR-034 stage 2): the viewed device's availability plus its device
/// generation. Frontends snapshot this from the projection they are about to
/// render, then fold it into the shell state — the pair is `Copy` precisely
/// so that read-then-fold sequence never holds a borrow of the snapshot it
/// read from.
#[derive(Clone, Copy, Debug)]
pub struct GpuChartMetricGate {
    /// Availability derived once from the viewed GPU's latest typed point.
    pub availability: GpuChartMetricAvailability,
    /// The viewed device's generation (`0` when no GPU is viewed — the fold
    /// itself is a no-op in that case, see
    /// [`GpuChartMetricSelection::reconcile_gate`]).
    pub generation: u64,
    /// Whether a GPU was viewed at all.
    pub viewed: bool,
}

impl GpuChartMetricGate {
    /// Snapshot the gate for the device a window is currently viewing.
    /// `None` yields the honest no-viewed-device gate the folds treat as a
    /// no-op.
    #[must_use]
    pub fn for_viewed_gpu(gpu: Option<&GpuMetrics>) -> Self {
        Self {
            availability: GpuChartMetricAvailability::for_viewed_gpu(gpu),
            generation: gpu.map_or(0, |gpu| gpu.device_generation.get()),
            viewed: gpu.is_some(),
        }
    }
}

/// The unit family one GPU chart series measures (from
/// [`GpuChartMetric::unit`]). Renderer-neutral so the three frontends map it
/// onto their own scale/badge types without disagreeing about the family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuChartMetricUnit {
    /// A 0..=100 percentage (utilization, memory families, idle residency).
    Percent,
    /// Board power draw in watts.
    Watts,
    /// Die temperature in °C.
    Celsius,
    /// Core clock in MHz.
    Megahertz,
}

/// Session-scoped chart-metric selection for the GPU headline chart
/// (ADR-034 decision 1): one instance per window, owned by the shell. Pure
/// value state — frontends must not persist it or derive a second copy
/// (ADR-034 验收约束: “选择态单一权威在 shell”).
///
/// Invariant after every transition: the selected family is available, or it
/// is the ADR default (which may itself be unavailable — the honest
/// dash/gap projection, never a swap to a fabricated choice).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuChartMetricSelection {
    selected: GpuChartMetric,
    /// Device generation the selection was last reconciled against. `None`
    /// until the first reconcile binds the viewed device; a real generation
    /// may legitimately be zero, so the unbound state is `None`, not `0`.
    generation: Option<u64>,
}

impl Default for GpuChartMetricSelection {
    fn default() -> Self {
        Self::new()
    }
}

impl GpuChartMetricSelection {
    /// The default selection: Utilization, no device binding yet
    /// (ADR-034 decision 1: “默认 Utilization”).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            selected: GpuChartMetric::DEFAULT,
            generation: None,
        }
    }

    /// The currently selected series family.
    #[must_use]
    pub const fn selected(&self) -> GpuChartMetric {
        self.selected
    }

    /// The device generation the selection was last reconciled against, or
    /// `None` before the first reconcile.
    #[must_use]
    pub const fn reconciled_generation(&self) -> Option<u64> {
        self.generation
    }

    /// Try to select one series family. Availability gates the vocabulary
    /// (ADR-034 decision 1: “序列可用性由 typed availability 门控”): an
    /// unavailable target is rejected with no state change — the disabled
    /// control a renderer draws for that family is a projection of this same
    /// boundary, not a second rule. Re-selecting the current family is a
    /// no-op. Returns whether the selection changed.
    pub fn select(
        &mut self,
        metric: GpuChartMetric,
        availability: &GpuChartMetricAvailability,
    ) -> bool {
        if self.selected == metric || !availability.is_available(metric) {
            return false;
        }
        self.selected = metric;
        true
    }

    /// Advance to the next series family in the fixed [`GpuChartMetric::ALL`]
    /// order, wrapping around (the TUI cycle key shape; ADR-034 stage 2
    /// wires the key itself). The cycle applies the same availability gate
    /// as [`Self::select`] and skips gate-rejected families
    /// deterministically, so it never lands on a series the next reconcile
    /// would fall back from. When nothing is available the cycle is a no-op.
    /// Returns whether the selection changed.
    pub fn cycle(&mut self, availability: &GpuChartMetricAvailability) -> bool {
        let order = GpuChartMetric::ALL;
        for step in 1..=order.len() {
            let candidate = order[(self.selected.index() + step) % order.len()];
            if availability.is_available(candidate) {
                if candidate == self.selected {
                    return false;
                }
                self.selected = candidate;
                return true;
            }
        }
        false
    }

    /// Reconcile against the viewed device's current availability and
    /// generation. A device-generation change resets the selection to the
    /// ADR default (ADR-034 stage-1 gate: “设备 generation 变化后的回退”); a
    /// selected family that became unavailable under the same generation
    /// falls back to the same default (the conservative reading of
    /// “不可用回退” — see the module docs). When even the default is
    /// unavailable the selection stays on the default and projects
    /// [`GpuChartMetricChoiceState::SelectedUnavailable`]. Returns whether
    /// the selection changed.
    pub fn reconcile(
        &mut self,
        availability: &GpuChartMetricAvailability,
        generation: u64,
    ) -> bool {
        // The first reconcile only BINDS the viewed device; it is not a
        // device change (ADR-034: “设备 generation 变化后的回退” — the
        // generation has to actually change), so a selection made on the
        // very first rendered frames survives the first fold.
        let generation_changed = self
            .generation
            .is_some_and(|previous| previous != generation);
        self.generation = Some(generation);
        if self.selected == GpuChartMetric::DEFAULT {
            return false;
        }
        if generation_changed || !availability.is_available(self.selected) {
            self.selected = GpuChartMetric::DEFAULT;
            return true;
        }
        false
    }

    /// Project the selector for renderers: the selected family plus every
    /// vocabulary entry with its explicit availability, in the fixed order.
    /// Unavailable entries stay present and explicit — never hidden, never
    /// zeroed (ADR-034 decision 1: “不可用序列投影为显式不可用，不伪造 0，也不
    /// 因单个序列不可用隐藏其余序列”). Stage-2 frontends render exactly this
    /// projection; they must not re-derive availability or keep a second
    /// selection.
    #[must_use]
    pub fn projection(
        &self,
        availability: &GpuChartMetricAvailability,
    ) -> GpuChartMetricProjection {
        GpuChartMetricProjection {
            selected: self.selected,
            choices: GpuChartMetric::ALL.map(|metric| GpuChartMetricChoice {
                metric,
                state: match (self.selected == metric, availability.is_available(metric)) {
                    (true, true) => GpuChartMetricChoiceState::Selected,
                    (false, true) => GpuChartMetricChoiceState::Selectable,
                    (false, false) => GpuChartMetricChoiceState::Unavailable,
                    (true, false) => GpuChartMetricChoiceState::SelectedUnavailable,
                },
            }),
        }
    }

    /// The per-tick fold the three frontends drive through their shell state
    /// (ADR-034 stage 2: “每 tick 折叠”): the viewed device's gate —
    /// availability from its latest typed point plus its generation —
    /// reconciled into the selection. A gate with no viewed device leaves
    /// the selection untouched (navigating away from the GPU surface is not
    /// a device change). Returns whether the selection changed.
    pub fn reconcile_gate(&mut self, gate: &GpuChartMetricGate) -> bool {
        gate.viewed && self.reconcile(&gate.availability, gate.generation)
    }

    /// Select through a viewed device's gate — the control path frontends
    /// expose for their pill/keyboard activation. A no-viewed-device gate
    /// rejects the selection.
    pub fn select_gate(&mut self, metric: GpuChartMetric, gate: &GpuChartMetricGate) -> bool {
        gate.viewed && self.select(metric, &gate.availability)
    }

    /// Cycle through a viewed device's gate (the TUI `g` key shape). A
    /// no-viewed-device gate is a no-op.
    pub fn cycle_gate(&mut self, gate: &GpuChartMetricGate) -> bool {
        gate.viewed && self.cycle(&gate.availability)
    }

    /// The selector projection through a viewed device's gate — the one
    /// projection every renderer paints. A no-viewed-device gate projects
    /// every family explicitly unavailable.
    #[must_use]
    pub fn projection_gate(&self, gate: &GpuChartMetricGate) -> GpuChartMetricProjection {
        self.projection(&gate.availability)
    }
}

/// Renderer-neutral selector projection (ADR-034 stage 2 consumes this).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuChartMetricProjection {
    /// The selected series family.
    pub selected: GpuChartMetric,
    /// Every vocabulary entry in fixed [`GpuChartMetric::ALL`] order.
    pub choices: [GpuChartMetricChoice; 8],
}

/// One selector entry: a series family plus its explicit state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuChartMetricChoice {
    pub metric: GpuChartMetric,
    pub state: GpuChartMetricChoiceState,
}

/// Explicit per-entry state for the selector controls and the headline
/// chart (ADR-034 验收约束: “不可用序列保持显式不可用投影（dash/gap）”).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuChartMetricChoiceState {
    /// Selected and available — render the series.
    Selected,
    /// Available but not selected — an enabled control.
    Selectable,
    /// Unavailable and not selected — the control stays visible and
    /// explicitly unavailable (dimmed/inert), never hidden.
    Unavailable,
    /// Selected but unavailable — the honest degradation: the chart draws
    /// the explicit dash/gap projection, not a fabricated zero and not a
    /// different series.
    SelectedUnavailable,
}

fn finite(value: Option<f32>) -> Option<f32> {
    value.filter(|value| value.is_finite())
}

/// Used/total percent with the guards the retired selector fold had: both
/// sides must exist, `used` clamps to `total`, and a zero capacity is
/// unavailable rather than a divide-by-zero percent.
fn bytes_percentage(used: Option<u64>, total: Option<u64>) -> Option<f32> {
    let (used, total) = (used?, total?);
    (total > 0).then(|| (used.min(total) as f64 * 100.0 / total as f64) as f32)
}

/// The live graph window for one GPU chart-metric family: ONE
/// generation-scoped typed read of the telemetry store's `gpu_metrics` ring,
/// folded per point by this module's [`GpuChartMetric::value`] — the same
/// fold the availability gate derives from, so the chart window and the gate
/// can never disagree about what one family carries. Every frontend consumes
/// this dispatch (Iced and the TUI through their shell history; GPUI through
/// its direct track's own `LiveGraphHistory` view of the same store), so
/// window capacity, gap and generation semantics have a single authority:
/// a missing scalar stays the `NaN` gap the ADR names for unavailable
/// stretches — never a fabricated zero — and a generation change breaks the
/// window (ring reset at ingest, plus the read-time generation scope).
#[must_use]
pub fn gpu_chart_metric_history(
    history: &LiveGraphHistory,
    device_id: &str,
    generation: u64,
    metric: GpuChartMetric,
) -> Vec<f32> {
    history.gpu_metric_point_series_for(device_id, generation, |point| metric.value(point))
}

#[cfg(test)]
#[path = "../../tests/headless/presentation_gpu_chart_metric.rs"]
mod tests;
