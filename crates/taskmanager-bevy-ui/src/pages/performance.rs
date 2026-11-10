//! Performance page — system summary, live curves, and per-device sections.
//!
//! Data entry follows the page-agent contract (see [`crate::pages`] and the
//! [`crate::app`] docs): every value renders from the read-only shell through
//! `context.shell`, style comes only from `context.palette`, and dynamic
//! refresh is observer-driven — never a per-frame poll:
//!
//! - **mount**: `scene::content` builds the static `bsn!` tree once from the
//!   current projection. Every value that can change later sits behind a
//!   self-describing marker (`DynText`, `SparkStrip`, `DynBlock`,
//!   `CurveGate`) naming the fact it renders.
//! - **bind**: composition registers typed page observers once. Unmounted
//!   frames have no marked components to update.
//! - **refresh**: `refresh_on_fold` re-reads the shell through
//!   `crate::app::ShellTrack` only when the drain actually folded batches,
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
//! is `metrics::observed_percentage` — no shared percent entry exists (the TUI keeps
//! its own copy in `ui/units.rs`), so this page owns one with the same shape.

use crate::navigation::{NavigationState, RAIL_WIDTH};
use bevy::ecs::component::Component;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{ParamSet, Query, Res, ResMut};
use bevy::scene::{Scene, bsn};
use bevy::ui::prelude::{
    AlignItems, Display, FlexDirection, JustifyContent, Node, Overflow, UiRect, Val, percent, px,
    rem,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::Activate;
use bevy::window::{PrimaryWindow, Window};
use taskforest_core::core::metrics::{
    CpuMetrics, CpuTelemetryObservation, DiskMetrics, GpuMetrics, GpuTelemetryObservation,
    MemoryMetrics, MemoryTelemetryObservation, NetworkMetrics, NetworkTelemetryObservation,
};
use taskforest_core::core::smart::SmartSelfTestKind;
use taskmanager_application::i18n::t;
use taskmanager_ui_contract::navigation::NavOrientation;

use taskmanager_shell::ShellApp;
use taskmanager_shell::memory::{MemSegment, MemSegmentKind, memory_segments, swap_breakdown};
use taskmanager_shell::presentation::GraphSummaryItem;
use taskmanager_shell::presentation::gpu_chart_metric::GpuChartMetric;
use taskmanager_shell::presentation::smart::self_test_intent;
use taskmanager_shell::presentation::trend::TrendSeries;
use taskmanager_shell::presentation::{
    bytes, gpu_display_identity, graph_summary, megahertz, missing_value, power_w, temperature_c,
};

use crate::app::{PageContext, ShellTrack};
use crate::palette::{UiPalette, space_2};
use crate::widgets::layout::{PerformanceLayoutMode, performance_sidebar_width};
use crate::window::{Role, TextRole, WindowPalette};

pub(crate) mod device_curves;
mod gpu_metric;
pub(crate) mod replay;
pub(crate) mod scene;
mod selection;
pub(crate) mod sidebar_network;
pub(crate) use selection::{PerformanceHeaderTarget, PerformanceStatsRailTarget};
pub(crate) mod sidebar_editor;

pub(crate) mod metrics;
mod refresh;

use crate::pages::history::control::PerformancePresentation;

pub(crate) use metrics::cpu::{CpuField, cpu_field_text};
use metrics::{
    curve_badge_text, curve_caption, curve_samples, curve_wanted, curve_warm, gpu_devices,
    gpu_fact_line, memory_metrics, network_devices, nic_fact_line, section_keys, segment_key,
};
use taskforest_core::core::percent::percent_text;

// ---- marker components (this page's private dynamic-state vocabulary) ----
//
// The `Default` seeds on value-carrying markers exist only for the bsn!
// template mechanism (template-then-patch); every spawned instance carries
// an explicit value.

/// Root of the mounted Performance page; typed observers update its markers.
#[derive(Component, Clone, Default)]
pub(crate) struct PerformancePageRoot;

/// Bounded device-fact viewport beneath fixed graph and selector controls.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct PerformanceDeviceBody;

/// The compact GPUI-style selector state. It is frontend-local presentation
/// state: the shell remains the authority for facts, while the selected curve
/// only decides which Bevy card gets the hero allocation.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PerformanceFocus(pub(crate) SystemCurve);

/// High-level device categories for performance view routing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DeviceCategoryKind {
    #[default]
    Cpu,
    Memory,
    Disk,
    Network,
    Gpu,
    Battery,
    Fan,
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DeviceViewCategory(pub(crate) DeviceCategoryKind);

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
    Battery(String),
    Fan(String),
}

impl PerformanceDeviceTarget {
    /// Top-level devices have a corresponding system curve. Per-disk/battery/fan focus
    /// intentionally returns `None` until the device-specific hero chart lands;
    /// selecting it still produces a real, visible local selection state.
    pub(crate) fn curve(&self) -> Option<SystemCurve> {
        match self {
            Self::Cpu => Some(SystemCurve::Cpu),
            Self::Memory => Some(SystemCurve::Memory),
            Self::Disk(_) | Self::Battery(_) | Self::Fan(_) => None,
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

/// Identity carried by one GPU chart-metric selector button. Activation drives
/// the shared shell selection (ADR-034), never a frontend-local metric state.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GpuMetricButton(pub(crate) GpuChartMetric);

impl Default for GpuMetricButton {
    fn default() -> Self {
        Self(GpuChartMetric::DEFAULT)
    }
}

/// Identity carried by one curve card so the focus observer can resize the
/// hero card without rebuilding any telemetry subtree.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CurveCard(pub(crate) SystemCurve);

/// The current responsive mode is layout state, not shell data. It changes
/// only when the primary window crosses the shared breakpoint and controls
/// which BSN-owned rails are visible.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PerformanceLayoutState(pub(crate) PerformanceLayoutMode);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PerformanceDeviceSidebar;

/// The presentation visibility of the performance device navigator.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PerformanceSidebarVisible(pub(crate) bool);

impl Default for PerformanceSidebarVisible {
    fn default() -> Self {
        Self(true)
    }
}

/// Triggered by F9 to toggle the device sidebar.
#[derive(Event, Clone, Copy, Debug)]
pub(crate) struct TogglePerformanceSidebar;

pub(crate) fn toggle_sidebar_observer(
    _event: On<TogglePerformanceSidebar>,
    mut sidebar: ResMut<PerformanceSidebarVisible>,
    mut device_rails: Query<&mut Node, With<PerformanceDeviceSidebar>>,
) {
    sidebar.0 = !sidebar.0;
    let display = if sidebar.0 {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut device_rails {
        node.display = display;
    }
}

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

/// One rewritable text node. The field names the fact it renders, so the
/// refresh observer is a flat query with no hierarchy walks.
#[derive(Component, Clone, Default)]
pub(crate) struct DynText(pub(crate) DynField);

/// One bar whose fill width is a CPU fact's magnitude (0–100%), keyed like
/// [`DynText`]. The fold observer rewrites the width from the same folded
/// projection the row's numeric text reads — two views, one observation.
/// The `Default` impl only exists for the bsn! template mechanism; the seed
/// is never a rendered fact.
#[derive(Component, Clone, Copy, Default)]
pub(crate) struct DynBar(pub(crate) CpuField);

/// One sparkline bar strip; the observer resizes (or rebuilds) its children
/// from the same shared projection the strip was spawned with.
#[derive(Component, Clone, Copy, Default)]
pub(crate) struct SparkStrip(pub(crate) SystemCurve);

/// One curve card's newest-value pill. The refresh observer rewrites its
/// text from the same finite sample window that drives the curve.
#[derive(Component, Clone, Copy, Default)]
pub(crate) struct CurveBadge(pub(crate) SystemCurve);

/// One container that owns a keyed dynamic block list (device sections, the
/// memory-composition rows).
#[derive(Component, Clone, Copy, Default)]
pub(crate) struct DynSection(pub(crate) Section);

/// Identity of one dynamic block: its owning section plus the stable key the
/// shell projection assigns (device id, segment kind). Presence follows the
/// projection — the block despawns when the key leaves it.
#[derive(Component, Clone, Default)]
pub(crate) struct DynBlock(pub(crate) Section, pub(crate) String);

/// Visibility marker for the disk SMART alert plate. The icon is mounted once
/// with the disk block and only its display token changes as the folded
/// projection changes, so a warning never becomes stale between ticks. The
/// plate covers every SMART crossing this page paints: the spare pool below
/// its warning level and the endurance past the device's rated life.
#[derive(Component, Clone, Default)]
pub(crate) struct DynDiskSmartAlert(pub(crate) String);

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
    Disk,
    Battery,
    Fan,
}

#[cfg_attr(not(test), allow(dead_code))]
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
    CurveCaptionItem(SystemCurve, GraphSummaryItem),
    CurveBadge(SystemCurve),
    GpuMetricLabel(String),
    SmartStatus(String),
    SmartGuidance(String),
    Cpu(CpuField),
    SidebarCaption1(PerformanceDeviceTarget),
    SidebarCaption2(PerformanceDeviceTarget),
    /// One device block's joined fact line, keyed by the stable device id.
    Device {
        section: Section,
        device: String,
    },
    Segment(MemSegmentKind),
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
    Npu,
}

impl SystemCurve {
    /// The strip order; the GPU and NPU cards are always spawned but display-gated.
    pub(crate) const STRIP: [Self; 5] =
        [Self::Cpu, Self::Memory, Self::Network, Self::Gpu, Self::Npu];

    fn series(self) -> TrendSeries {
        match self {
            Self::Cpu => TrendSeries::CpuUsagePercent,
            Self::Memory => TrendSeries::MemoryUsagePercent,
            Self::Network => TrendSeries::NetworkBytesPerSec,
            Self::Gpu => TrendSeries::GpuUsagePercent,
            Self::Npu => TrendSeries::NpuUsagePercent,
        }
    }

    /// Value spelling inside the curve caption: percent curves read like the
    /// TUI graphs; the throughput curve reuses the shared byte formatter.
    fn format_value(self, value: f32) -> String {
        match self {
            Self::Network => format!("{}/s", bytes(value.max(0.0) as u64)),
            _ => percent_text(value),
        }
    }

    /// Bar ink per curve, from the palette only (never a literal).
    fn color(self, palette: &UiPalette) -> bevy::color::Color {
        match self {
            Self::Cpu => palette.cpu,
            Self::Memory => palette.memory,
            Self::Network => palette.network,
            Self::Gpu | Self::Npu => palette.accent,
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
        SystemCurve::Npu => "NPU",
    }
    .to_owned()
}

/// Arm the SMART self-test confirmation gate for the specified disk.
pub(crate) fn request_smart_self_test(
    shell: &mut ShellApp,
    disk_id: &str,
    kind: SmartSelfTestKind,
) -> bool {
    let Some(disks) = shell
        .projection()
        .snapshot
        .as_ref()
        .map(|snapshot| &snapshot.disks)
    else {
        return false;
    };
    let Some(disk) = disks.iter().find(|d| d.device_id == disk_id) else {
        return false;
    };
    self_test_intent(disk, kind).is_some_and(|intent| shell.arm_smart_self_test(intent))
}

// ---- view model: pure resolvers over the read-only shell ----

// ---- scene composition ----
//
// The `bsn!` builders live in the `performance::scene` submodule (split for
// the per-file source budget); `content` there is the page-agent entry.

// ---- dynamic refresh: the ShellProjectionFolded observer ----

pub(crate) fn register(app: &mut bevy::app::App) {
    app.init_resource::<PerformanceFocus>();
    app.init_resource::<PerformanceDeviceFocus>();
    app.add_observer(refresh::refresh_on_fold);
    gpu_metric::register(app);
    selection::register(app);
    device_curves::register(app);
    sidebar_editor::register(app);
    metrics::battery::register(app);
    // The battery icon's charge flow: the band travels the fill while the
    // observed charge state says the pack is charging.
    scene::battery::register(app);
    // The core grid's reveal is a PER-FRAME motion: the fold observer only
    // flips the typed target, and the animator eases the height towards it.
    // Without this system the reveal would ride the ~1 Hz telemetry cadence
    // and read as a jump (BEVY-115).
    app.add_systems(
        bevy::app::PostUpdate,
        animate_core_grid.after(bevy::ui::UiSystems::PostLayout),
    );
}

/// The per-frame core-grid animator: the column policy follows the measured
/// width, and the reveal eases towards its typed target.
fn animate_core_grid(
    time: Res<bevy::time::Time>,
    palette: Res<WindowPalette>,
    mut surface: crate::pages::performance::scene::sidebar::cores::CoreGridSurface,
) {
    crate::pages::performance::scene::sidebar::cores::apply_core_grid_columns(
        &palette.inner,
        &surface.rows,
        &mut surface.cells,
    );
    crate::pages::performance::scene::sidebar::cores::advance_core_grid(
        time.elapsed_secs(),
        &palette.inner,
        &surface.toggles,
        &surface.contents,
        &mut surface.bodies,
    );
}

/// Bevy 0.20's official button widget emits `Activate` for pointer and
/// keyboard activation. Resolve the typed marker, update the local focus and
/// publish one presentation event; no shell effect or telemetry request is
/// involved.
fn focus_button_activated(
    activate: On<Activate>,
    buttons: Query<&PerformanceFocusButton>,
    track: ShellTrack,
    mut focus: ResMut<PerformanceFocus>,
    mut device_focus: ResMut<PerformanceDeviceFocus>,
) {
    let Ok(button) = buttons.get(activate.event().entity) else {
        return;
    };
    let target = match button.0 {
        SystemCurve::Cpu | SystemCurve::Npu => Some(PerformanceDeviceTarget::Cpu),
        SystemCurve::Memory => Some(PerformanceDeviceTarget::Memory),
        SystemCurve::Network => network_devices(track.shell())
            .and_then(|devices| devices.first())
            .map(|device| PerformanceDeviceTarget::Network(device.device_id.to_string())),
        SystemCurve::Gpu => gpu_devices(track.shell())
            .and_then(|devices| devices.first())
            .map(|device| PerformanceDeviceTarget::Gpu(device.device_id.clone())),
    };
    if let Some(target) = target {
        device_focus.0 = target;
    }
    focus.0 = button.0;
}

/// Bevy 0.20's official button widget emits `Activate` for the compact
/// device pills as well. Top-level device targets share the existing curve
/// focus so the selector and hero card stay in one local presentation state;
/// disk targets remain selectable without pretending a disk hero chart exists.
fn device_button_activated(
    activate: On<Activate>,
    buttons: Query<&PerformanceDeviceButton>,
    mut device_focus: ResMut<PerformanceDeviceFocus>,
    mut curve_focus: ResMut<PerformanceFocus>,
) {
    let button = buttons
        .get(activate.entity)
        .or_else(|_| buttons.get(activate.event().entity));
    let Ok(button) = button else {
        return;
    };
    if device_focus.0 == button.0 {
        return;
    }
    device_focus.0 = button.0.clone();
    if let Some(curve) = button.0.curve()
        && curve_focus.0 != curve
    {
        curve_focus.0 = curve;
    }
}

/// Apply the shared responsive contract to scene-owned rails. The system is
/// deliberately geometry-only: telemetry remains observer-driven and no page
/// subtree is rebuilt when a window crosses the breakpoint.
pub(crate) fn sync_performance_layout(
    windows: Query<&Window, bevy::ecs::query::With<PrimaryWindow>>,
    navigation: Option<Res<NavigationState>>,
    sidebar: Option<Res<PerformanceSidebarVisible>>,
    replay: Option<Res<PerformancePresentation>>,
    preferences: Option<Res<crate::pages::performance::sidebar_editor::SidebarState>>,
    mut state: ResMut<PerformanceLayoutState>,
    mut rails: ParamSet<(
        DeviceRailQuery<'_, '_>,
        StatsRailQuery<'_, '_>,
        WideNavQuery<'_, '_>,
        CompactNavQuery<'_, '_>,
        CompactPillsQuery<'_, '_>,
    )>,
) {
    let width = windows.iter().next().map_or(1180.0, Window::width)
        - if navigation
            .as_ref()
            .is_some_and(|navigation| navigation.0 == NavOrientation::Vertical)
        {
            RAIL_WIDTH
        } else {
            0.0
        };
    let mode = crate::widgets::layout::performance_layout_mode(width);
    state.0 = mode;
    let replay_on = replay
        .as_deref()
        .is_some_and(|presentation| *presentation == PerformancePresentation::Replay);
    let sidebar_on = sidebar.is_none_or(|s| s.0) && !replay_on;
    let display = match mode {
        PerformanceLayoutMode::Wide if sidebar_on => Display::Flex,
        _ => Display::None,
    };
    // The sidebar claims only the surplus the budget leaves after the main
    // column and the statistics rail keep their floors.
    let sidebar_width = performance_sidebar_width(
        preferences.as_ref().map_or(0.0, |p| p.0.sidebar_width),
        width,
    );
    for mut node in &mut rails.p0() {
        node.display = display;
        let wanted = px(sidebar_width);
        if node.width != wanted {
            node.width = wanted;
        }
        if node.min_width != wanted {
            node.min_width = wanted;
        }
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

/// The marker query surface a folded projection rewrites.
///
/// Grouped so the observer's parameter list stays its dependency list: the
/// marker queries name the mounted page's nodes, while `nodes`/`transforms`
/// are the shared node/transform queries the strip and gate adapters reuse.
#[cfg(test)]
#[path = "../../tests/headless/pages/performance.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/headless/pages/performance_bounds_probe.rs"]
mod bounds_probe_tests;

#[cfg(test)]
#[path = "../../tests/headless/pages/performance_cpu.rs"]
mod cpu_details_tests;

#[cfg(test)]
#[path = "../../tests/headless/pages/performance_device_rows.rs"]
mod device_rows_tests;

#[cfg(test)]
#[path = "../../tests/headless/pages/performance_thermal.rs"]
mod thermal_status_tests;

#[cfg(test)]
#[path = "../../tests/headless/pages/performance_selection.rs"]
mod selection_tests;

#[cfg(test)]
#[path = "../../tests/headless/pages/performance_device_history.rs"]
mod device_history_tests;

#[cfg(test)]
#[path = "../../tests/headless/pages/performance_optical.rs"]
mod optical_tests;
