//! Performance page — system summary, live curves, and per-device sections.
//!
//! Data entry follows the page-agent contract (see [`crate::pages`] and the
//! [`crate::app`] docs): every value renders from the read-only shell through
//! `context.shell`, style comes only from `context.palette`, and dynamic
//! refresh is observer-driven — never a per-frame poll:
//!
//! - **mount**: [`scene::content`] builds the static `bsn!` tree once from the
//!   current projection. Every value that can change later sits behind a
//!   self-describing marker ([`DynText`], [`SparkStrip`], [`DynBlock`],
//!   [`CurveGate`]) naming the fact it renders.
//! - **bind**: the root's `on_insert` hook registers this page's observer on
//!   [`crate::drain::ShellProjectionFolded`] exactly once per `World`
//!   (guarded by a resource), keeping the page module self-contained — no
//!   shared-file edit, and unmounted frames do zero work because the markers
//!   no longer exist.
//! - **refresh**: [`refresh_on_fold`] re-reads the shell through
//!   [`crate::app::ShellTrack`] only when the drain actually folded batches,
//!   rewrites texts in place (equality-guarded so identical facts are not
//!   change-detected), resizes sparkline bars while the sample count still
//!   matches (rebuilding only when the window warms or the capacity changes),
//!   and adds/removes device blocks so a vanished device's block vanishes
//!   with it — identity is the shell's stable device id; this page caches
//!   nothing.
//!
//! Behavior semantics follow the TUI performance surfaces (the exemplar): a
//! curve warms at two samples (`perf.collecting_samples` before that, never a
//! fabricated flat line or zero), missing observations render the shared dash,
//! the memory/swap breakdown comes from `taskmanager_shell::memory` (the
//! saturating single source), and byte/temperature/power/clock strings come
//! from `taskmanager_shell::presentation` (ADR-020). The one local formatter
//! is [`observed_percentage`] — no shared percent entry exists (the TUI keeps
//! its own copy in `ui/units.rs`), so this page owns one with the same shape.

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, ParamSet, Query, Res, ResMut};
use bevy::ecs::world::{DeferredWorld, World};
use bevy::scene::{CommandsSceneExt, Scene, bsn};
use bevy::ui::prelude::{
    AlignItems, Display, FlexDirection, JustifyContent, Node, Overflow, UiRect, Val, percent, px,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, ScrollArea};
use bevy::window::{PrimaryWindow, Window};
use taskmanager_application::i18n::t;
use taskmanager_application::{
    CpuMetrics, CpuTelemetryObservation, DiskMetrics, GpuMetrics, GpuTelemetryObservation,
    MemoryMetrics, MemoryTelemetryObservation, NetworkMetrics, NetworkTelemetryObservation,
    SystemTelemetryDomainState,
};
use taskmanager_shell::ShellApp;
use taskmanager_shell::history::MetricSeries;
use taskmanager_shell::memory::{MemSegment, MemSegmentKind, memory_segments, swap_breakdown};
use taskmanager_shell::presentation::{
    bytes, gpu_display_identity, graph_summary, megahertz, missing_value, power_w, temperature_c,
};

use crate::app::{PageContext, ShellTrack};
use crate::drain::ShellProjectionFolded;
use crate::palette::{UiPalette, space_2};
use crate::widgets::chart::{ChartSurface, MAX_CHART_POINTS, line_segments};
use crate::widgets::controls::ControlVisual;
use crate::widgets::layout::PerformanceLayoutMode;
use crate::widgets::sparkline::bar_fractions;
use crate::window::{Role, TextRole, WindowPalette};

pub(crate) mod scene;
use scene::{bar_height, bar_scene, block_scene};

// ---- marker components (this page's private dynamic-state vocabulary) ----
//
// The `Default` seeds on value-carrying markers exist only for the bsn!
// template mechanism (template-then-patch); every spawned instance carries
// an explicit value.

/// Root of the mounted Performance page. Its `on_insert` hook binds the
/// refresh observer, so mounting the page is what activates its data path.
#[derive(Component, Clone, Default)]
#[component(on_insert = bind_refresh_observer)]
pub(crate) struct PerformancePageRoot;

/// The compact GPUI-style selector state. It is frontend-local presentation
/// state: the shell remains the authority for facts, while the selected curve
/// only decides which Bevy card gets the hero allocation.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PerformanceFocus(pub(crate) SystemCurve);

/// A compact device selector identity. It is frontend-local presentation
/// state: the shell still owns the device facts and stable ids, while this
/// value only chooses which already-mounted surface receives emphasis.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum PerformanceDeviceTarget {
    #[default]
    Cpu,
    Memory,
    Disk(String),
    Network(String),
    Gpu(String),
}

impl PerformanceDeviceTarget {
    /// Top-level devices have a corresponding system curve. Per-disk focus
    /// intentionally returns `None` until the disk-specific hero chart lands;
    /// selecting it still produces a real, visible local selection state.
    fn curve(&self) -> Option<SystemCurve> {
        match self {
            Self::Cpu => Some(SystemCurve::Cpu),
            Self::Memory => Some(SystemCurve::Memory),
            Self::Disk(_) => None,
            Self::Network(_) => Some(SystemCurve::Network),
            Self::Gpu(_) => Some(SystemCurve::Gpu),
        }
    }
}

/// The selected device identity in the Performance presentation. This never
/// crosses into application commands or shell state.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PerformanceDeviceFocus(pub(crate) PerformanceDeviceTarget);

/// Identity carried by one metric selector button. The button is a declarative
/// `bsn!` scene; activation only changes the presentation focus resource.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PerformanceFocusButton(pub(crate) SystemCurve);

/// Identity carried by a compact device selector button.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PerformanceDeviceButton(pub(crate) PerformanceDeviceTarget);

/// Identity carried by one curve card so the focus observer can resize the
/// hero card without rebuilding any telemetry subtree.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CurveCard(pub(crate) SystemCurve);

#[derive(Event, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PerformanceFocusChanged(pub(crate) SystemCurve);

#[derive(Event, Clone, Debug, PartialEq, Eq)]
pub(crate) struct PerformanceDeviceFocusChanged(pub(crate) PerformanceDeviceTarget);

/// The current responsive mode is layout state, not shell data. It changes
/// only when the primary window crosses the shared breakpoint and controls
/// which BSN-owned rails are visible.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PerformanceLayoutState(pub(crate) PerformanceLayoutMode);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PerformanceDeviceSidebar;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PerformanceStatsRail;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PerformanceWideNav;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PerformanceCompactNav;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PerformanceCompactDevicePills;

type DeviceRailQuery<'w, 's> = Query<'w, 's, &'w mut Node, With<PerformanceDeviceSidebar>>;
type StatsRailQuery<'w, 's> = Query<'w, 's, &'w mut Node, With<PerformanceStatsRail>>;
type WideNavQuery<'w, 's> = Query<'w, 's, &'w mut Node, With<PerformanceWideNav>>;
type CompactNavQuery<'w, 's> = Query<'w, 's, &'w mut Node, With<PerformanceCompactNav>>;
type CompactPillsQuery<'w, 's> = Query<'w, 's, &'w mut Node, With<PerformanceCompactDevicePills>>;

/// Once-per-`World` guard so remounts never stack duplicate observers (two
/// observers on one trigger would run before either's spawn commands apply
/// and could double-spawn blocks).
#[derive(Resource, Default)]
struct RefreshObserverBound;

/// One rewritable text node. The field names the fact it renders, so the
/// refresh observer is a flat query with no hierarchy walks.
#[derive(Component, Clone, Default)]
pub(crate) struct DynText(pub(crate) DynField);

/// One sparkline bar strip; the observer resizes (or rebuilds) its children
/// from the same shared projection the strip was spawned with.
#[derive(Component, Clone, Copy, Default)]
pub(crate) struct SparkStrip(pub(crate) SystemCurve);

/// One container that owns a keyed dynamic block list (device sections, the
/// memory-composition rows).
#[derive(Component, Clone, Copy, Default)]
pub(crate) struct DynSection(pub(crate) Section);

/// Identity of one dynamic block: its owning section plus the stable key the
/// shell projection assigns (device id, segment kind). Presence follows the
/// projection — the block despawns when the key leaves it.
#[derive(Component, Clone, Default)]
pub(crate) struct DynBlock(pub(crate) Section, pub(crate) String);

/// One curve card root. The CPU/memory/network cards are always wanted; the
/// GPU card stays `Display::None` until GPU facts exist ("add GPU when data
/// exists" — a host without GPU telemetry never shows an empty fourth card).
#[derive(Component, Clone, Copy, Default)]
pub(crate) struct CurveGate(pub(crate) SystemCurve);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Section {
    #[default]
    Gpu,
    Network,
    MemorySegments,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SummaryField {
    #[default]
    Cpu,
    Cores,
    Memory,
    Swap,
    NetReceive,
    NetSend,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DynField {
    Summary(SummaryField),
    CurveCaption(SystemCurve),
    Cpu(CpuField),
    /// One device block's joined fact line, keyed by the stable device id.
    Device {
        section: Section,
        device: String,
    },
    Segment(MemSegmentKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CpuField {
    Brand,
    Usage,
    Frequency,
    Temperature,
    Power,
    Core(usize),
}

impl Default for DynField {
    fn default() -> Self {
        Self::Summary(SummaryField::Cpu)
    }
}

/// One system-wide curve the curve strip renders.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SystemCurve {
    #[default]
    Cpu,
    Memory,
    Network,
    Gpu,
}

impl SystemCurve {
    /// The strip order; the GPU card is always spawned but display-gated.
    pub(crate) const STRIP: [Self; 4] = [Self::Cpu, Self::Memory, Self::Network, Self::Gpu];

    fn series(self) -> MetricSeries {
        match self {
            Self::Cpu => MetricSeries::CpuUsagePercent,
            Self::Memory => MetricSeries::MemoryUsagePercent,
            Self::Network => MetricSeries::NetworkBytesPerSec,
            Self::Gpu => MetricSeries::GpuUsagePercent,
        }
    }

    fn title(self) -> String {
        match self {
            Self::Cpu => t("common.cpu"),
            Self::Memory => t("common.memory"),
            Self::Network => t("perf.system_network_throughput"),
            Self::Gpu => t("perf.system_gpu_utilization"),
        }
        .to_owned()
    }

    /// Value spelling inside the curve caption: percent curves read like the
    /// TUI graphs; the throughput curve reuses the shared byte formatter.
    fn format_value(self, value: f32) -> String {
        match self {
            Self::Network => format!("{}/s", bytes(value.max(0.0) as u64)),
            _ => format!("{value:.0}%"),
        }
    }

    /// Bar ink per curve, from the palette only (never a literal).
    fn color(self, palette: &UiPalette) -> bevy::color::Color {
        match self {
            Self::Cpu | Self::Gpu => palette.accent,
            Self::Memory => palette.body_color,
            Self::Network => palette.dim_color,
        }
    }
}

/// Short labels for the GPUI-style metric selector. The full graph title stays
/// on the card; pills remain bounded so compact windows never clip a control.
pub(crate) fn curve_selector_label(curve: SystemCurve) -> String {
    match curve {
        SystemCurve::Cpu => t("common.cpu"),
        SystemCurve::Memory => t("common.memory"),
        SystemCurve::Network => t("sidebar.network"),
        SystemCurve::Gpu => t("common.gpu"),
    }
    .to_owned()
}

// ---- view model: pure resolvers over the read-only shell ----

/// Percent readout. There is no shared percent formatter in
/// `shell::presentation` (the TUI keeps its own in `ui/units.rs`), so this
/// page owns one with the same semantics: missing and non-finite observations
/// render the shared dash, never a fabricated `0.0%`.
fn observed_percentage(value: Option<f32>) -> String {
    value
        .filter(|value| value.is_finite())
        .map_or_else(missing_value, |value| format!("{value:.1}%"))
}

/// A domain observation's usable value: current/partial read the live value,
/// stale/unavailable keep the last known one — `None` stays `None` all the
/// way to a dash, never a zero.
fn usable_value<'a, T, V>(
    state: &'a SystemTelemetryDomainState<T>,
    current: fn(&'a T) -> Option<V>,
    last_known: fn(&'a T) -> Option<V>,
) -> Option<V> {
    match state {
        SystemTelemetryDomainState::Current(observation)
        | SystemTelemetryDomainState::Partial(observation) => current(observation),
        SystemTelemetryDomainState::Stale(observation)
        | SystemTelemetryDomainState::Unavailable {
            observation: Some(observation),
            ..
        } => last_known(observation),
        _ => None,
    }
}

/// Read the correlated six-domain projection first. The complete shell
/// snapshot is the honest fallback for demo/cold-start frames: it is already
/// the shell's committed render model, so Performance does not show dashes
/// merely because the newer partial telemetry stream has not warmed yet.
fn cpu_metrics(shell: &ShellApp) -> Option<&CpuMetrics> {
    shell
        .projection()
        .system_telemetry
        .as_ref()
        .and_then(|telemetry| {
            usable_value(
                &telemetry.cpu,
                CpuTelemetryObservation::current_value,
                CpuTelemetryObservation::last_known_value,
            )
        })
        .or_else(|| {
            shell
                .projection()
                .snapshot
                .as_ref()
                .map(|snapshot| &snapshot.cpu)
        })
}

fn memory_metrics(shell: &ShellApp) -> Option<&MemoryMetrics> {
    shell
        .projection()
        .system_telemetry
        .as_ref()
        .and_then(|telemetry| {
            usable_value(
                &telemetry.memory,
                MemoryTelemetryObservation::current_value,
                MemoryTelemetryObservation::last_known_value,
            )
        })
        .or_else(|| {
            shell
                .projection()
                .snapshot
                .as_ref()
                .map(|snapshot| &snapshot.memory)
        })
}

fn gpu_devices(shell: &ShellApp) -> Option<&[GpuMetrics]> {
    shell
        .projection()
        .system_telemetry
        .as_ref()
        .and_then(|telemetry| {
            usable_value(
                &telemetry.gpu,
                GpuTelemetryObservation::current_value,
                GpuTelemetryObservation::last_known_value,
            )
        })
        .or_else(|| {
            shell
                .projection()
                .snapshot
                .as_ref()
                .map(|snapshot| snapshot.gpu.as_slice())
        })
}

fn network_devices(shell: &ShellApp) -> Option<&[NetworkMetrics]> {
    shell
        .projection()
        .system_telemetry
        .as_ref()
        .and_then(|telemetry| {
            usable_value(
                &telemetry.network,
                NetworkTelemetryObservation::current_value,
                NetworkTelemetryObservation::last_known_value,
            )
        })
        .or_else(|| {
            shell
                .projection()
                .snapshot
                .as_ref()
                .map(|snapshot| snapshot.networks.as_slice())
        })
}

pub(crate) fn summary_value(shell: &ShellApp, field: SummaryField) -> String {
    match field {
        SummaryField::Cpu => {
            observed_percentage(cpu_metrics(shell).and_then(|cpu| cpu.current_global_usage_pct()))
        }
        SummaryField::Cores => core_summary(cpu_metrics(shell)),
        SummaryField::Memory => memory_summary(memory_metrics(shell)),
        // `swap_breakdown` only answers when a positive total is configured,
        // so `None` (no swap) is the dash, never "0 / 0".
        SummaryField::Swap => {
            memory_metrics(shell)
                .and_then(swap_breakdown)
                .map_or_else(missing_value, |swap| {
                    let pct = (swap.used_bytes as f64 / swap.total_bytes as f64 * 100.0)
                        .clamp(0.0, 100.0);
                    format!(
                        "{} / {} ({pct:.0}%)",
                        bytes(swap.used_bytes),
                        bytes(swap.total_bytes)
                    )
                })
        }
        SummaryField::NetReceive => network_rate(
            network_devices(shell),
            NetworkMetrics::current_rx_bytes_per_sec,
        ),
        SummaryField::NetSend => network_rate(
            network_devices(shell),
            NetworkMetrics::current_tx_bytes_per_sec,
        ),
    }
}

/// Per-core usages, one readout per projected core with honest dashes for
/// per-core gaps; no cores observed at all renders the plain dash.
fn core_summary(cpu: Option<&CpuMetrics>) -> String {
    let Some(cpu) = cpu else {
        return missing_value();
    };
    let count = cpu.current_core_usage_len();
    if count == 0 {
        return missing_value();
    }
    (0..count)
        .map(|index| observed_percentage(cpu.current_core_usage_pct(index)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// "used / total · pct" with a dash per missing side; the percentage comes
/// from the core's own observed fold, never recomputed here.
fn memory_summary(memory: Option<&MemoryMetrics>) -> String {
    let Some(memory) = memory else {
        return missing_value();
    };
    let used = memory.current_used_bytes().map(bytes);
    let total = memory.current_total_bytes().map(bytes);
    let percentage = memory
        .used_percentage_observed()
        .filter(|value| value.is_finite())
        .map(|value| format!("{value:.1}%"));
    let line = format!(
        "{} / {}",
        used.unwrap_or_else(missing_value),
        total.unwrap_or_else(missing_value)
    );
    match percentage {
        Some(percentage) => format!("{line} · {percentage}"),
        None => line,
    }
}

/// Sum one rate direction across the projected adapters. An empty or absent
/// list — or any adapter missing the fact — stays a dash: a partial sum (or
/// a zero over nothing) would read as the system total it is not.
fn network_rate(
    devices: Option<&[NetworkMetrics]>,
    rate: fn(&NetworkMetrics) -> Option<u64>,
) -> String {
    let Some(devices) = devices.filter(|devices| !devices.is_empty()) else {
        return missing_value();
    };
    devices
        .iter()
        .map(rate)
        .collect::<Option<Vec<u64>>>()
        .map_or_else(missing_value, |rates| {
            format!("{}/s", bytes(rates.iter().sum()))
        })
}

fn curve_samples(shell: &ShellApp, curve: SystemCurve) -> Vec<f32> {
    shell.history.series(curve.series())
}

/// TUI parity: a window under two samples is still collecting — the curve
/// area shows the collecting placeholder, never a fabricated flat line.
fn curve_warm(samples: &[f32]) -> bool {
    samples.len() >= 2
}

pub(crate) fn curve_caption(shell: &ShellApp, curve: SystemCurve) -> String {
    let samples = curve_samples(shell, curve);
    if !curve_warm(&samples) {
        return t("perf.collecting_samples").to_owned();
    }
    graph_summary(&samples).map_or_else(missing_value, |summary| {
        format!(
            "{} {} · {} {} · {} {}",
            t("common.latest"),
            curve.format_value(summary.latest),
            t("common.avg"),
            curve.format_value(summary.average),
            t("common.peak"),
            curve.format_value(summary.maximum),
        )
    })
}

fn cpu_field_text(shell: &ShellApp, field: CpuField) -> String {
    let Some(cpu) = cpu_metrics(shell) else {
        return missing_value();
    };
    match field {
        CpuField::Brand => cpu
            .brand
            .as_deref()
            .map(str::trim)
            .filter(|brand| !brand.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(missing_value),
        CpuField::Usage => observed_percentage(cpu.current_global_usage_pct()),
        CpuField::Frequency => cpu
            .current_frequency_mhz()
            .map(|value| megahertz(value as f32))
            .unwrap_or_else(missing_value),
        CpuField::Temperature => cpu
            .current_temperature_c()
            .filter(|value| value.is_finite())
            .map(temperature_c)
            .unwrap_or_else(missing_value),
        CpuField::Power => cpu
            .current_power_w()
            .filter(|value| value.is_finite())
            .map(power_w)
            .unwrap_or_else(missing_value),
        CpuField::Core(index) => observed_percentage(cpu.current_core_usage_pct(index)),
    }
}

/// Bar fractions for one strip: the shared sparkline projection, emptied
/// while the window is not warm (the strip renders nothing rather than a
/// fake line).
pub(crate) fn strip_fractions(shell: &ShellApp, curve: SystemCurve) -> Vec<f32> {
    let samples = curve_samples(shell, curve);
    if curve_warm(&samples) {
        bar_fractions(&samples)
    } else {
        Vec::new()
    }
}

fn curve_wanted(shell: &ShellApp, curve: SystemCurve) -> bool {
    match curve {
        SystemCurve::Gpu => gpu_devices(shell).is_some_and(|devices| !devices.is_empty()),
        _ => true,
    }
}

fn segment_key(kind: MemSegmentKind) -> String {
    format!("{kind:?}")
}

/// Ordered block keys for one section: the shell projection's device list
/// (stable device ids) or the shared memory segment kinds.
pub(crate) fn section_keys(shell: &ShellApp, section: Section) -> Vec<String> {
    match section {
        Section::Gpu => gpu_devices(shell).map_or_else(Vec::new, |devices| {
            devices.iter().map(|gpu| gpu.device_id.clone()).collect()
        }),
        Section::Network => network_devices(shell).map_or_else(Vec::new, |devices| {
            devices
                .iter()
                .map(|nic| (*nic.device_id).to_owned())
                .collect()
        }),
        Section::MemorySegments => memory_metrics(shell).map_or_else(Vec::new, |memory| {
            memory_segments(memory)
                .iter()
                .map(|segment| segment_key(segment.kind))
                .collect()
        }),
    }
}

/// One GPU block's joined fact line; each fact keeps its own dash-on-missing
/// semantics (TUI `gpu_data` parity via the shared formatters).
fn gpu_fact_line(gpu: &GpuMetrics) -> String {
    [
        observed_percentage(gpu.current_utilization_pct()),
        gpu.current_temperature_c()
            .filter(|value| value.is_finite())
            .map_or_else(missing_value, temperature_c),
        gpu.current_frequency_mhz()
            .map_or_else(missing_value, |mhz| megahertz(mhz as f32)),
        gpu.current_power_w()
            .filter(|value| value.is_finite())
            .map_or_else(missing_value, power_w),
        gpu_memory_line(gpu),
    ]
    .join(" · ")
}

fn nic_fact_line(nic: &NetworkMetrics) -> String {
    let rate = |value: Option<u64>| {
        value.map_or_else(missing_value, |value| format!("{}/s", bytes(value)))
    };
    [
        rate(nic.current_rx_bytes_per_sec()),
        rate(nic.current_tx_bytes_per_sec()),
        nic.current_link_speed_mbps()
            .map_or_else(missing_value, |mbps| format!("{mbps} Mbps")),
    ]
    .join(" · ")
}

/// A device block's current fact line from the projection; a device id that
/// left the projection renders the dash (its block is being despawned).
pub(crate) fn device_line(shell: &ShellApp, section: Section, device: &str) -> String {
    match section {
        Section::Gpu => gpu_devices(shell)
            .and_then(|devices| devices.iter().find(|gpu| gpu.device_id == device))
            .map_or_else(missing_value, gpu_fact_line),
        Section::Network => network_devices(shell)
            .and_then(|devices| devices.iter().find(|nic| &*nic.device_id == device))
            .map_or_else(missing_value, nic_fact_line),
        Section::MemorySegments => missing_value(),
    }
}

/// First present VRAM pair (dedicated, then shared, then general) rendered as
/// "used / total"; a pair needs a positive total — an absent counter is not a
/// believable zero capacity (TUI `gpu_data` parity).
fn gpu_memory_line(gpu: &GpuMetrics) -> String {
    let vram_pair = |used: Option<u64>, total: Option<u64>| -> Option<(u64, u64)> {
        match (used, total) {
            (Some(used), Some(total)) if total > 0 => Some((used.min(total), total)),
            _ => None,
        }
    };
    let pair = vram_pair(
        gpu.current_dedicated_vram_used_bytes(),
        gpu.current_dedicated_vram_total_bytes(),
    )
    .or_else(|| {
        vram_pair(
            gpu.current_shared_vram_used_bytes(),
            gpu.current_shared_vram_total_bytes(),
        )
    })
    .or_else(|| {
        vram_pair(
            gpu.current_memory_used_bytes(),
            gpu.current_memory_total_bytes(),
        )
    });
    pair.map_or_else(missing_value, |(used, total)| {
        format!("{} / {}", bytes(used), bytes(total))
    })
}

pub(crate) fn segment_value(shell: &ShellApp, kind: MemSegmentKind) -> String {
    let Some(memory) = memory_metrics(shell) else {
        return missing_value();
    };
    memory_segments(memory)
        .iter()
        .find(|segment| segment.kind == kind)
        .map_or_else(missing_value, |segment| {
            segment_line(segment, memory.current_total_bytes())
        })
}

/// One composition legend row: byte count plus its clamped share of a known
/// positive total (the segment math itself — which categories exist and
/// their saturating byte sums — is owned by `taskmanager_shell::memory`).
fn segment_line(segment: &MemSegment, total: Option<u64>) -> String {
    let share = total.filter(|total| *total > 0).map(|total| {
        let pct = (segment.bytes as f64 / total as f64 * 100.0).clamp(0.0, 100.0);
        format!("{pct:.0}%")
    });
    share.map_or_else(
        || bytes(segment.bytes),
        |share| format!("{} · {share}", bytes(segment.bytes)),
    )
}

fn dyn_field_text(shell: &ShellApp, field: &DynField) -> String {
    match field {
        DynField::Summary(field) => summary_value(shell, *field),
        DynField::CurveCaption(curve) => curve_caption(shell, *curve),
        DynField::Cpu(field) => cpu_field_text(shell, *field),
        DynField::Device { section, device } => device_line(shell, *section, device),
        DynField::Segment(kind) => segment_value(shell, *kind),
    }
}

// ---- scene composition ----
//
// The `bsn!` builders live in the `performance::scene` submodule (split for
// the per-file source budget); `content` there is the page-agent entry.

// ---- dynamic refresh: the ShellProjectionFolded observer ----

/// `on_insert` hook for [`PerformancePageRoot`]: register the page's refresh
/// observer once per `World`. Registration happens in a queued exclusive
/// command so the check-and-bind pair is atomic against remounts.
fn bind_refresh_observer(mut world: DeferredWorld, _context: HookContext) {
    world.commands().queue(|world: &mut World| {
        if world.get_resource::<RefreshObserverBound>().is_some() {
            return;
        }
        world.init_resource::<PerformanceFocus>();
        world.init_resource::<PerformanceDeviceFocus>();
        world.insert_resource(RefreshObserverBound);
        world.add_observer(refresh_on_fold);
        world.add_observer(sync_focus_changed);
        world.add_observer(sync_device_focus_changed);
    });
}

/// Bevy 0.19's official button widget emits `Activate` for pointer and
/// keyboard activation. Resolve the typed marker, update the local focus and
/// publish one presentation event; no shell effect or telemetry request is
/// involved.
fn focus_button_activated(
    activate: On<Activate>,
    buttons: Query<&PerformanceFocusButton>,
    mut focus: ResMut<PerformanceFocus>,
    mut commands: Commands,
) {
    let Ok(button) = buttons.get(activate.event().entity) else {
        return;
    };
    if focus.0 == button.0 {
        return;
    }
    focus.0 = button.0;
    commands.trigger(PerformanceFocusChanged(button.0));
}

/// Bevy 0.19's official button widget emits `Activate` for the compact
/// device pills as well. Top-level device targets share the existing curve
/// focus so the selector and hero card stay in one local presentation state;
/// disk targets remain selectable without pretending a disk hero chart exists.
fn device_button_activated(
    activate: On<Activate>,
    buttons: Query<&PerformanceDeviceButton>,
    mut device_focus: ResMut<PerformanceDeviceFocus>,
    mut curve_focus: ResMut<PerformanceFocus>,
    mut commands: Commands,
) {
    let Ok(button) = buttons.get(activate.event().entity) else {
        return;
    };
    if device_focus.0 == button.0 {
        return;
    }
    device_focus.0 = button.0.clone();
    commands.trigger(PerformanceDeviceFocusChanged(button.0.clone()));
    if let Some(curve) = button.0.curve()
        && curve_focus.0 != curve
    {
        curve_focus.0 = curve;
        commands.trigger(PerformanceFocusChanged(curve));
    }
}

/// Apply the selector's active surface and hero-card allocation in place.
/// This is a bounded presentation update: the scene tree remains stable and
/// all telemetry text/chart markers keep their existing entities.
fn sync_focus_changed(
    _changed: On<PerformanceFocusChanged>,
    focus: Res<PerformanceFocus>,
    mut buttons: Query<(&PerformanceFocusButton, &mut ControlVisual)>,
    mut cards: Query<(&CurveCard, &mut Node)>,
) {
    for (button, mut visual) in &mut buttons {
        visual.1 = button.0 == focus.0;
    }
    for (card, mut node) in &mut cards {
        node.flex_grow = if card.0 == focus.0 { 2.0 } else { 1.0 };
        node.display = if card.0 == focus.0 {
            Display::Flex
        } else {
            Display::None
        };
    }
}

/// Apply the selected device token to every compact pill in place. The pills
/// remain a stable BSN scene; only their theme-backed fill changes.
fn sync_device_focus_changed(
    _changed: On<PerformanceDeviceFocusChanged>,
    focus: Res<PerformanceDeviceFocus>,
    mut buttons: Query<(&PerformanceDeviceButton, &mut ControlVisual)>,
) {
    for (button, mut visual) in &mut buttons {
        visual.1 = button.0 == focus.0;
    }
}

/// Apply the shared responsive contract to scene-owned rails. The system is
/// deliberately geometry-only: telemetry remains observer-driven and no page
/// subtree is rebuilt when a window crosses the breakpoint.
pub(crate) fn sync_performance_layout(
    windows: Query<&Window, bevy::ecs::query::With<PrimaryWindow>>,
    mut state: ResMut<PerformanceLayoutState>,
    mut rails: ParamSet<(
        DeviceRailQuery<'_, '_>,
        StatsRailQuery<'_, '_>,
        WideNavQuery<'_, '_>,
        CompactNavQuery<'_, '_>,
        CompactPillsQuery<'_, '_>,
    )>,
) {
    let width = windows.iter().next().map_or(1180.0, Window::width);
    let mode = crate::widgets::layout::performance_layout_mode(width);
    state.0 = mode;
    let display = match mode {
        PerformanceLayoutMode::Wide => Display::Flex,
        PerformanceLayoutMode::Compact => Display::None,
    };
    for mut node in &mut rails.p0() {
        node.display = display;
    }
    for mut node in &mut rails.p1() {
        node.display = display;
    }
    for mut node in &mut rails.p2() {
        node.display = match mode {
            PerformanceLayoutMode::Wide => Display::Flex,
            PerformanceLayoutMode::Compact => Display::None,
        };
    }
    for mut node in &mut rails.p3() {
        node.display = match mode {
            PerformanceLayoutMode::Wide => Display::None,
            PerformanceLayoutMode::Compact => Display::Flex,
        };
    }
    for mut node in &mut rails.p4() {
        node.display = match mode {
            PerformanceLayoutMode::Wide => Display::None,
            PerformanceLayoutMode::Compact => Display::Flex,
        };
    }
}

/// The pages' data-refresh trigger consumer (see [`crate::drain`]): re-read
/// the folded shell and push new facts into the mounted tree. Runs only when
/// the drain folded batches — an idle frame never reaches this observer, and
/// a frame with the page unmounted finds no markers and does nothing.
// Bevy observers declare their data access as parameters, so the param count
// is the observer's dependency list, not function-design sprawl (the same
// policy as the gpui graph builders).
#[allow(clippy::too_many_arguments)]
fn refresh_on_fold(
    _fold: On<ShellProjectionFolded>,
    track: ShellTrack,
    palette: Res<WindowPalette>,
    focus: Res<PerformanceFocus>,
    mut texts: Query<(&DynText, &mut Text)>,
    mut strips: Query<(Entity, &SparkStrip, &Children, &mut ChartSurface)>,
    gates: Query<(Entity, &CurveGate)>,
    sections: Query<(Entity, &DynSection)>,
    blocks: Query<(Entity, &DynBlock)>,
    mut nodes: Query<&mut Node>,
    mut commands: Commands,
) {
    let shell = track.shell();
    rewrite_texts(shell, &mut texts);
    sync_strips(
        shell,
        &palette.inner,
        &mut strips,
        &mut nodes,
        &mut commands,
    );
    sync_card_gates(shell, focus.0, &gates, &mut nodes);
    sync_blocks(shell, &palette.inner, &sections, &blocks, &mut commands);
}

/// Rewrite every marked text from the current projection, skipping values
/// that did not change (no change detection on identical facts).
fn rewrite_texts(shell: &ShellApp, texts: &mut Query<(&DynText, &mut Text)>) {
    for (field, mut text) in texts.iter_mut() {
        let wanted = dyn_field_text(shell, &field.0);
        if text.0 != wanted {
            text.0 = wanted;
        }
    }
}

/// Resize each strip's bars from the current window. A matching child count
/// rewrites heights in place (the warm steady state re-spawns nothing); a
/// changed count rebuilds the bar list once.
fn sync_strips(
    shell: &ShellApp,
    palette: &UiPalette,
    strips: &mut Query<(Entity, &SparkStrip, &Children, &mut ChartSurface)>,
    nodes: &mut Query<&mut Node>,
    commands: &mut Commands,
) {
    for (strip_entity, strip, children, mut chart) in strips.iter_mut() {
        let segment_count = line_segments(
            &shell.history.series(strip.0.series()),
            100.0,
            palette.control_height_px * 2.0,
            MAX_CHART_POINTS,
        )
        .len();
        chart.0 = segment_count;
        let fractions = strip_fractions(shell, strip.0);
        if children.len() == fractions.len() {
            for (child, fraction) in children.iter().zip(fractions.iter()) {
                let height = bar_height(*fraction, palette);
                if let Ok(mut node) = nodes.get_mut(*child)
                    && node.height != Val::Px(height)
                {
                    node.height = Val::Px(height);
                }
            }
            continue;
        }
        for child in children.iter() {
            commands.entity(*child).despawn();
        }
        for fraction in &fractions {
            let bar = commands
                .spawn_scene(bar_scene(
                    bar_height(*fraction, palette),
                    strip.0.color(palette),
                ))
                .id();
            commands
                .entity(strip_entity)
                .add_one_related::<ChildOf>(bar);
        }
    }
}

/// Flip each card's gate with its curve's data existence (only the GPU card
/// can close), touching only nodes whose display actually changes.
fn sync_card_gates(
    shell: &ShellApp,
    focus: SystemCurve,
    gates: &Query<(Entity, &CurveGate)>,
    nodes: &mut Query<&mut Node>,
) {
    let active = if curve_wanted(shell, focus) {
        focus
    } else {
        SystemCurve::default()
    };
    for (entity, gate) in gates.iter() {
        let wanted = if curve_wanted(shell, gate.0) && gate.0 == active {
            Display::Flex
        } else {
            Display::None
        };
        if let Ok(mut node) = nodes.get_mut(entity)
            && node.display != wanted
        {
            node.display = wanted;
        }
    }
}

/// Reconcile each section's blocks with the projection's key list: despawn
/// keys that left, spawn keys that arrived. The projection is the only
/// memory of which devices exist — the page keeps no cache.
fn sync_blocks(
    shell: &ShellApp,
    palette: &UiPalette,
    sections: &Query<(Entity, &DynSection)>,
    blocks: &Query<(Entity, &DynBlock)>,
    commands: &mut Commands,
) {
    for (section_entity, section) in sections.iter() {
        let desired = section_keys(shell, section.0);
        for (block_entity, block) in blocks.iter() {
            if block.0 == section.0 && !desired.iter().any(|key| key == &block.1) {
                commands.entity(block_entity).despawn();
            }
        }
        for key in desired {
            if blocks
                .iter()
                .any(|(_, block)| block.0 == section.0 && block.1 == key)
            {
                continue;
            }
            if let Some(scene) = block_scene(section.0, &key, shell, palette) {
                let child = commands.spawn_scene(scene).id();
                commands
                    .entity(section_entity)
                    .add_one_related::<ChildOf>(child);
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/headless/pages/performance.rs"]
mod tests;
