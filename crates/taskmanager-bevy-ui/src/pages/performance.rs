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

mod metrics;

use metrics::{
    cpu_field_text, cpu_metrics, curve_caption, curve_samples, curve_wanted, curve_warm,
    dyn_field_text, gpu_devices, gpu_fact_line, memory_metrics, network_devices, nic_fact_line,
    section_keys, segment_key, segment_line, strip_fractions, summary_value,
};
use scene::blocks::block_scene;
use scene::chart::{bar_height, bar_scene};

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
