//! test-intent: behavior
//!
//! Headless behavior tests for the Performance page.
//!
//! Three clusters, all against the REAL shell fold (no window, no
//! compositor):
//! - the pure view-model resolvers: an empty or partially-missing projection
//!   renders the shared dash and the collecting placeholder — never a
//!   fabricated zero or flat line;
//! - the curve wiring: the strip and caption follow the shared sparkline
//!   projection and the TUI's two-sample warm rule;
//! - the wired page on `MinimalPlugins` + the real window plugin: folding a
//!   real `PlatformEventBatch` into the `FrontendTrack` shell and triggering
//!   the drain's `ShellProjectionFolded` must rewrite the mounted scene
//!   (summary values, curve bars) and reconcile the device blocks with the
//!   projection's device list — a vanished device's block vanishes with it,
//!   and an unmounted page has no markers left to touch.

use std::collections::BTreeMap;
use std::sync::Arc;

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use bevy::picking::hover::PickingInteraction;
use bevy::scene::{ScenePlugin, WorldSceneExt};
use bevy::text::Font;
use bevy::ui::BackgroundColor;
use bevy::ui::Node;
use bevy::ui::widget::Text;
use bevy::ui_widgets::Activate;
use taskmanager_application::i18n::t;
use taskmanager_application::{
    CorrelatedSystemTelemetryOutcome, HostTelemetryRequest, PlatformClient, PlatformEvent,
    PlatformEventBatch, PlatformFacets, PlatformHandle, ProjectedSystemTelemetry, SystemFacets,
    SystemTelemetryDomainEvent, SystemTelemetryDomainOutcome, SystemTelemetryDomainState,
    SystemTelemetryRevision,
};
use taskmanager_core::core::metrics::MemoryTelemetryObservation;
use taskmanager_core::core::metrics::{
    CpuMetrics, CpuScalarObservations, CpuTelemetryObservation, MemoryMetrics,
    MemoryScalarObservations, NetworkAdapterType, NetworkMetrics, NetworkScalarObservations,
    NetworkTelemetryObservation, NetworkWirelessObservations, ScalarObservation,
    ScalarObservationGroup,
};
use taskmanager_platform_contract::{
    CapabilityCatalog, CapabilityDescriptor, CapabilityId, CapabilitySnapshot, CapabilityStatus,
    EventEnvelope, EventPort, EventPortError, EventSequence, RequestId, RequestPort,
    SubmissionError,
};

use taskmanager_shell::presentation::MISSING_VALUE;
use taskmanager_shell::{ShellApp, demo_app};
use taskmanager_theme::Theme;

use super::scene::content;
use super::{
    CurveCard, CurveGate, DynBlock, DynField, DynText, PerformanceDeviceButton,
    PerformanceDeviceFocus, PerformanceDeviceTarget, PerformanceFocus, PerformanceFocusButton,
    Section, SparkStrip, SummaryField, SystemCurve, curve_caption, curve_wanted, section_keys,
    summary_value,
};
use crate::app::{FrontendTrack, Page, PageContext, Route, RouteChanged};
use crate::drain::ShellProjectionFolded;
use crate::palette::ui_palette;
use crate::runtime::{RuntimeCache, SharedRuntime};
use crate::widgets::chart::{MAX_CHART_POINTS, line_segments};
use crate::window::FrontendWindowPlugin;
use crate::window::tests::HeadlessFrontendPlugins;

const GIB: u64 = 1024 * 1024 * 1024;

/// The strip's polyline projection over the shell's series — the same
/// bounded, gap-aware call the render path makes (design strip geometry).
fn curve_segments(
    shell: &taskmanager_shell::ShellApp,
    curve: SystemCurve,
) -> Vec<crate::widgets::chart::ChartSegment> {
    line_segments(
        &taskmanager_shell::presentation::trend::window(&shell.history, curve.series()),
        super::scene::chart::CHART_STRIP_WIDTH_PX,
        34.0 * 3.0,
        MAX_CHART_POINTS,
    )
}

// ---- typed telemetry fixtures (real application shapes, no mocks) ----

fn cpu_metrics(usage_pct: f32, cores: &[f32], at_ms: u64) -> CpuMetrics {
    let mut observations = CpuScalarObservations {
        global_usage_pct: ScalarObservation::available(usage_pct, at_ms),
        ..CpuScalarObservations::default()
    };
    if !cores.is_empty() {
        observations.core_usage_group = ScalarObservationGroup::available(cores.to_vec(), at_ms);
    }
    CpuMetrics::from_observations(observations)
}

fn memory_metrics(
    at_ms: u64,
    used: u64,
    total: u64,
    available: u64,
    swap: (u64, u64),
) -> MemoryMetrics {
    let base = MemoryMetrics::default();
    MemoryMetrics::from_observations(
        MemoryScalarObservations {
            total_bytes: ScalarObservation::available(total, at_ms),
            used_bytes: ScalarObservation::available(used, at_ms),
            available_bytes: ScalarObservation::available(available, at_ms),
            swap_total_bytes: ScalarObservation::available(swap.1, at_ms),
            swap_used_bytes: ScalarObservation::available(swap.0, at_ms),
            ..MemoryScalarObservations::default()
        },
        base.optional_observations().clone(),
    )
}

fn nic(key: &str, interface: &str, at_ms: u64, rx_per_sec: u64, tx_per_sec: u64) -> NetworkMetrics {
    let mut nic = NetworkMetrics::new(interface);
    nic.device_id = Arc::from(key);
    nic.apply_observations(
        NetworkAdapterType::Other,
        NetworkScalarObservations {
            rx_bytes_per_sec: ScalarObservation::available(rx_per_sec, at_ms),
            tx_bytes_per_sec: ScalarObservation::available(tx_per_sec, at_ms),
            ..NetworkScalarObservations::default()
        },
        NetworkWirelessObservations::default(),
    );
    nic
}

fn correlated(
    sequence: u64,
    at_ms: u64,
    capability: CapabilityId,
    event: SystemTelemetryDomainOutcome,
) -> CorrelatedSystemTelemetryOutcome {
    CorrelatedSystemTelemetryOutcome {
        request_id: RequestId::MIN,
        capability,
        provider: None,
        sequence: EventSequence::new(sequence),
        observed_at_ms: at_ms,
        event,
    }
}

fn cpu_outcome(sequence: u64, at_ms: u64, metrics: CpuMetrics) -> CorrelatedSystemTelemetryOutcome {
    correlated(
        sequence,
        at_ms,
        CapabilityId::TELEMETRY_CPU,
        SystemTelemetryDomainOutcome::Observed(SystemTelemetryDomainEvent::Cpu {
            revision: SystemTelemetryRevision::new(sequence),
            observation: Box::new(CpuTelemetryObservation::current(metrics, at_ms, Vec::new())),
        }),
    )
}

fn network_outcome(
    sequence: u64,
    at_ms: u64,
    devices: Vec<NetworkMetrics>,
) -> CorrelatedSystemTelemetryOutcome {
    correlated(
        sequence,
        at_ms,
        CapabilityId::TELEMETRY_NETWORK,
        SystemTelemetryDomainOutcome::Observed(SystemTelemetryDomainEvent::Network {
            revision: SystemTelemetryRevision::new(sequence),
            observation: Box::new(NetworkTelemetryObservation::current(
                devices,
                at_ms,
                Vec::new(),
                Vec::new(),
                BTreeMap::new(),
            )),
        }),
    )
}

/// One projection revision: the resolved domains carry current observations,
/// the rest stay pending (the merge policy accepts that shape).
fn projection(
    revision: u64,
    cpu: CpuMetrics,
    memory: MemoryMetrics,
    nics: Vec<NetworkMetrics>,
) -> ProjectedSystemTelemetry {
    ProjectedSystemTelemetry {
        revision: SystemTelemetryRevision::new(revision),
        cpu: SystemTelemetryDomainState::Current(CpuTelemetryObservation::current(
            cpu,
            1,
            Vec::new(),
        )),
        memory: SystemTelemetryDomainState::Current(MemoryTelemetryObservation::current(
            memory,
            1,
            Vec::new(),
        )),
        network: SystemTelemetryDomainState::Current(NetworkTelemetryObservation::current(
            nics,
            1,
            Vec::new(),
            Vec::new(),
            BTreeMap::new(),
        )),
        host: SystemTelemetryDomainState::default(),
        storage: SystemTelemetryDomainState::default(),
        gpu: SystemTelemetryDomainState::default(),
    }
}

/// A shell plus a monotonically advancing sequence stamp for real folds.
struct Folded {
    shell: ShellApp,
    sequence: u64,
}

impl Folded {
    fn new() -> Self {
        Self {
            shell: ShellApp::new(),
            sequence: 0,
        }
    }

    fn apply(
        &mut self,
        outcomes: Vec<CorrelatedSystemTelemetryOutcome>,
        projection: ProjectedSystemTelemetry,
    ) {
        let sequence = self.sequence + 1;
        self.sequence = sequence;
        let batch = PlatformEventBatch {
            system_telemetry_outcomes: outcomes,
            system_telemetry_projections: vec![projection],
            ..PlatformEventBatch::default()
        };
        self.shell.apply_platform_batch(batch);
    }
}

/// One host fold: CPU (summary value + one curve sample), memory (summary +
/// composition), and the given NIC list (device blocks + summed rates).
fn fold_host(folded: &mut Folded, usage: f32, nics: Vec<NetworkMetrics>) {
    let at = 1_000 + folded.sequence * 100;
    let sequence = folded.sequence + 1;
    let cpu = cpu_metrics(usage, &[12.0, 4.0, 98.0], at);
    let memory = memory_metrics(at, 4 * GIB, 16 * GIB, 12 * GIB, (GIB, 4 * GIB));
    let outcomes = vec![
        cpu_outcome(sequence, at, cpu.clone()),
        network_outcome(sequence, at, nics.clone()),
    ];
    folded.apply(outcomes, projection(sequence, cpu, memory, nics));
}

// ---- pure resolvers: honest placeholders, never fabricated zeros ----

#[test]
fn empty_shell_renders_dashes_and_collecting_not_zeros() {
    let folded = Folded::new();
    let shell = &folded.shell;
    for field in [
        SummaryField::Cpu,
        SummaryField::Cores,
        SummaryField::Memory,
        SummaryField::Swap,
        SummaryField::NetReceive,
        SummaryField::NetSend,
    ] {
        assert_eq!(
            summary_value(shell, field),
            MISSING_VALUE,
            "an empty projection must not fabricate a value for {field:?}"
        );
    }
    for curve in SystemCurve::STRIP {
        assert_eq!(
            curve_caption(shell, curve),
            t("perf.collecting_samples"),
            "a cold curve window is the honest collecting state"
        );
        assert!(
            curve_segments(shell, curve).is_empty(),
            "a cold strip draws no segments, not a fake flat line"
        );
    }
    assert!(section_keys(shell, Section::Gpu).is_empty());
    assert!(section_keys(shell, Section::Network).is_empty());
    assert!(section_keys(shell, Section::MemorySegments).is_empty());
}

#[test]
fn partial_projection_keeps_missing_domains_on_dashes() {
    let mut folded = Folded::new();
    fold_host(&mut folded, 42.5, Vec::new());
    let shell = &folded.shell;
    // CPU facts arrived: their values render.
    assert_eq!(summary_value(shell, SummaryField::Cpu), "42.5%");
    assert_eq!(
        summary_value(shell, SummaryField::Cores),
        "12.0% 4.0% 98.0%"
    );
    // An empty NIC list is not a measured zero-total: the summed rates stay
    // dashes while the memory facts did land.
    assert_eq!(
        summary_value(shell, SummaryField::NetReceive),
        MISSING_VALUE
    );
    assert_eq!(summary_value(shell, SummaryField::NetSend), MISSING_VALUE);
    assert_eq!(
        summary_value(shell, SummaryField::Memory),
        "4.0 GiB / 16.0 GiB · 25.0%"
    );
    assert_eq!(
        summary_value(shell, SummaryField::Swap),
        "1.0 GiB / 4.0 GiB (25%)"
    );
    // The shell's saturating segment math yields the two-segment fallback
    // (in-use + available) for this observation shape.
    assert_eq!(section_keys(shell, Section::MemorySegments).len(), 2);
}

// ---- curve wiring: two-sample warm rule + shared polyline projection ----

#[test]
fn curve_warms_at_two_samples_like_the_tui() {
    let mut folded = Folded::new();
    fold_host(&mut folded, 10.0, Vec::new());
    assert!(
        curve_segments(&folded.shell, SystemCurve::Cpu).is_empty(),
        "one sample is still collecting (TUI parity: no fabricated line)"
    );
    assert_eq!(
        curve_caption(&folded.shell, SystemCurve::Cpu),
        t("perf.collecting_samples")
    );

    fold_host(&mut folded, 50.0, Vec::new());
    fold_host(&mut folded, 90.0, Vec::new());
    let segments = curve_segments(&folded.shell, SystemCurve::Cpu);
    assert_eq!(segments.len(), 2, "one connecting segment per sample pair");
    // Screen y grows downward: an ascending sample window must render an
    // ascending polyline (each segment ends higher than it starts), never a
    // flat or inverted line.
    for segment in &segments {
        assert!(
            segment.end.y < segment.start.y,
            "ascending samples draw an ascending line: {segments:?}"
        );
    }
    let caption = curve_caption(&folded.shell, SystemCurve::Cpu);
    assert!(
        caption.contains("90%"),
        "latest sample in caption: {caption}"
    );
    assert!(caption.contains("50%"), "average in caption: {caption}");
}

// ---- the wired page: real app composition, real fold, real trigger ----

struct FixedCapabilities(CapabilitySnapshot);

impl CapabilityCatalog for FixedCapabilities {
    fn snapshot(&self) -> CapabilitySnapshot {
        self.0.clone()
    }
}

struct QuietEvents;

impl EventPort for QuietEvents {
    type Event = PlatformEvent;

    fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError> {
        Ok(None)
    }
}

struct QuietRequests;

impl RequestPort for QuietRequests {
    type Request = HostTelemetryRequest;

    fn try_submit(
        &self,
        _request: taskmanager_platform_contract::RequestEnvelope<Self::Request>,
    ) -> Result<(), SubmissionError> {
        Ok(())
    }
}

fn descriptor(id: CapabilityId, status: CapabilityStatus) -> CapabilityDescriptor {
    CapabilityDescriptor {
        id,
        status,
        providers: Vec::new(),
        observed_at_ms: 1,
        last_success_at_ms: None,
    }
}

fn scripted_runtime() -> &'static SharedRuntime {
    let snapshot = CapabilitySnapshot::from_descriptors([descriptor(
        CapabilityId::TELEMETRY_HOST,
        CapabilityStatus::Available,
    )]);
    let client = PlatformClient::new(PlatformHandle::new(
        Arc::new(FixedCapabilities(snapshot)),
        Arc::new(QuietEvents),
        PlatformFacets::default()
            .with_system(SystemFacets::default().with_host(Arc::new(QuietRequests))),
    ));
    let cache: &'static RuntimeCache = Box::leak(Box::new(RuntimeCache::new()));
    cache
        .get_or_init(move || Ok(client))
        .expect("the scripted runtime always starts")
}

/// `MinimalPlugins` + the real window plugin (real route/mount systems and
/// the real drain), with the font store `AssetPlugin` does not auto-create.
fn headless_perf_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(HeadlessFrontendPlugins);
    app.add_plugins(FrontendWindowPlugin {
        runtime: scripted_runtime(),
        palette: ui_palette(&Theme::dark()),
    });
    app.init_resource::<Assets<Font>>();
    app
}

/// Route to the Performance page the programmatic way (route resource move +
/// `RouteChanged` trigger — the same pair the keyboard adapter performs).
fn route_to_performance(app: &mut App) {
    app.world_mut().resource_mut::<Route>().page = Page::Performance;
    app.world_mut()
        .commands()
        .trigger(RouteChanged(Page::Performance));
    app.update();
    app.update();
}

/// Fold one real platform batch into the frontend track's shell, then fire
/// the drain's data-refresh trigger exactly the way `drain_system` does.
fn fold_and_trigger(app: &mut App, batch: PlatformEventBatch) {
    app.world_mut()
        .non_send_mut::<FrontendTrack>()
        .shell
        .apply_platform_batch(batch);
    app.world_mut().commands().trigger(ShellProjectionFolded(1));
    app.update();
    app.update();
}

fn host_batch(sequence: u64, usage: f32, nics: Vec<NetworkMetrics>) -> PlatformEventBatch {
    let at = 1_000 + sequence * 100;
    let cpu = cpu_metrics(usage, &[12.0, 4.0, 98.0], at);
    let memory = memory_metrics(at, 4 * GIB, 16 * GIB, 12 * GIB, (GIB, 4 * GIB));
    PlatformEventBatch {
        system_telemetry_outcomes: vec![
            cpu_outcome(sequence, at, cpu.clone()),
            network_outcome(sequence, at, nics.clone()),
        ],
        system_telemetry_projections: vec![projection(sequence, cpu, memory, nics)],
        ..PlatformEventBatch::default()
    }
}

fn dyn_text_value(world: &mut World, field: &DynField) -> Option<String> {
    let mut texts = world.query::<(&DynText, &Text)>();
    texts
        .iter(world)
        .find(|(marker, _)| marker.0 == *field)
        .map(|(_, text)| text.0.clone())
}

fn block_keys(world: &mut World, section: Section) -> Vec<String> {
    let mut blocks = world.query::<&DynBlock>();
    let mut keys: Vec<String> = blocks
        .iter(world)
        .filter(|block| block.0 == section)
        .map(|block| block.1.clone())
        .collect();
    keys.sort();
    keys
}

#[test]
fn content_spawns_from_a_cold_context_with_strip_markers() {
    // The page assembles from a cold shell (the shared page-assembly gate
    // spawns every page this way) and every curve strip exists as a marker
    // the refresh observer can bind to, with zero bars while cold.
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    app.init_resource::<Assets<Font>>();
    let shell = ShellApp::new();
    let palette = ui_palette(&Theme::dark());
    let history = crate::pages::history::HistoryProjectionResource::default();
    let process_tree_expansion = crate::pages::process_tree::ProcessTreeExpansion::default();
    let context = PageContext {
        shell: &shell,
        process_tree_expansion: &process_tree_expansion,
        palette: &palette,
        body: palette.body.clone(),
        heading: palette.heading.clone(),
        history: &history.0,
    };
    let world = app.world_mut();
    let root = world
        .spawn_scene(content(&context))
        .expect("the cold page scene resolves without assets")
        .id();
    let mut strips = world.query::<(&SparkStrip, &bevy::ecs::hierarchy::Children)>();
    for curve in SystemCurve::STRIP {
        let strip = strips
            .iter(world)
            .find(|(marker, _)| marker.0 == curve)
            .unwrap_or_else(|| panic!("the {curve:?} strip is mounted with its marker"));
        assert_eq!(
            strip.1.len(),
            1,
            "a cold strip mounts exactly its polyline layer"
        );
        let segments = strip
            .1
            .iter()
            .next()
            .and_then(|polyline| world.get::<bevy::ecs::hierarchy::Children>(*polyline))
            .map(|segments| segments.len())
            .unwrap_or(usize::MAX);
        assert_eq!(
            segments, 0,
            "a cold strip draws zero segments, never a fabricated line"
        );
    }
    assert!(world.despawn(root), "the cold page scene despawns cleanly");
}

#[test]
fn demo_snapshot_fallback_keeps_facts_and_real_gpu_presence() {
    // The demo fixture seeds the committed snapshot, but not the live
    // system-telemetry resource. Performance must still show those facts at
    // cold start; the fallback is a read-only projection bridge, not a
    // fabricated zero/default.
    let shell = demo_app();
    assert_eq!(summary_value(&shell, SummaryField::Cpu), "37.4%");
    assert_ne!(summary_value(&shell, SummaryField::Memory), MISSING_VALUE);
    assert_eq!(section_keys(&shell, Section::Gpu).len(), 1);
    assert!(curve_wanted(&shell, SystemCurve::Gpu));
}

#[test]
fn metric_selector_activation_promotes_one_bsn_card_in_place() {
    let mut app = headless_perf_app();
    app.update();
    route_to_performance(&mut app);

    let memory_button = {
        let world = app.world_mut();
        let mut buttons = world.query::<(Entity, &PerformanceFocusButton)>();
        buttons
            .iter(world)
            .find(|(_, button)| button.0 == SystemCurve::Memory)
            .map(|(entity, _)| entity)
            .expect("the memory selector is a bsn scene button")
    };
    app.world_mut().commands().trigger(Activate {
        entity: memory_button,
    });
    app.update();
    app.update();

    assert_eq!(
        app.world().resource::<PerformanceFocus>().0,
        SystemCurve::Memory,
        "Activate updates only the frontend-local presentation focus"
    );

    let (memory_grow, cpu_grow, memory_display, cpu_display) = {
        let world = app.world_mut();
        let mut cards = world.query::<(&CurveCard, &Node)>();
        let mut memory = None;
        let mut cpu = None;
        for (card, node) in cards.iter(world) {
            match card.0 {
                SystemCurve::Memory => memory = Some((node.flex_grow, node.display)),
                SystemCurve::Cpu => cpu = Some((node.flex_grow, node.display)),
                _ => {}
            }
        }
        let (memory_grow, memory_display) = memory.expect("the memory card remains mounted");
        let (cpu_grow, cpu_display) = cpu.expect("the CPU card remains mounted");
        (memory_grow, cpu_grow, memory_display, cpu_display)
    };
    assert_eq!(
        memory_grow, 2.0,
        "the selected card receives the hero share"
    );
    assert_eq!(cpu_grow, 1.0, "the non-selected card keeps the base share");
    assert_eq!(
        memory_display,
        bevy::ui::Display::Flex,
        "the selected card is the only visible hero surface"
    );
    assert_eq!(
        cpu_display,
        bevy::ui::Display::None,
        "the unselected card stays mounted but leaves the compact viewport"
    );

    let expected_active = ui_palette(&Theme::dark()).nav_active_bg;
    let memory_fill = {
        let world = app.world_mut();
        let mut buttons = world.query::<(&PerformanceFocusButton, &BackgroundColor)>();
        buttons
            .iter(world)
            .find(|(button, _)| button.0 == SystemCurve::Memory)
            .map(|(_, fill)| fill.0)
            .expect("the memory selector keeps its marker and fill")
    };
    assert_eq!(
        memory_fill, expected_active,
        "the active selector uses the shared theme token"
    );
}

#[test]
fn compact_device_activation_updates_local_state_and_shared_curve_focus() {
    let mut app = headless_perf_app();
    app.update();
    route_to_performance(&mut app);

    let memory_button = {
        let world = app.world_mut();
        let mut buttons = world.query::<(Entity, &PerformanceDeviceButton)>();
        buttons
            .iter(world)
            .find(|(_, button)| button.0 == PerformanceDeviceTarget::Memory)
            .map(|(entity, _)| entity)
            .expect("the compact memory selector is a bsn scene button")
    };
    app.world_mut().commands().trigger(Activate {
        entity: memory_button,
    });
    app.update();
    app.update();

    assert_eq!(
        app.world().resource::<PerformanceDeviceFocus>().0,
        PerformanceDeviceTarget::Memory,
        "device activation changes only frontend-local selection state"
    );
    assert_eq!(
        app.world().resource::<PerformanceFocus>().0,
        SystemCurve::Memory,
        "top-level device activation reuses the existing curve focus authority"
    );

    let expected_active = ui_palette(&Theme::dark()).nav_active_bg;
    let memory_fill = {
        let world = app.world_mut();
        let mut buttons = world.query::<(&PerformanceDeviceButton, &BackgroundColor)>();
        buttons
            .iter(world)
            .find(|(button, _)| button.0 == PerformanceDeviceTarget::Memory)
            .map(|(_, fill)| fill.0)
            .expect("the memory device pill keeps its marker and fill")
    };
    assert_eq!(
        memory_fill, expected_active,
        "the active device pill uses the shared theme token"
    );
}

#[test]
fn performance_controls_paint_hover_pressed_and_selected_tokens() {
    let mut app = headless_perf_app();
    app.update();
    route_to_performance(&mut app);

    let memory_button = {
        let world = app.world_mut();
        let mut buttons = world.query::<(Entity, &PerformanceFocusButton)>();
        buttons
            .iter(world)
            .find(|(_, button)| button.0 == SystemCurve::Memory)
            .map(|(entity, _)| entity)
            .expect("the memory curve selector is mounted as a button")
    };
    let palette = ui_palette(&Theme::dark());

    app.world_mut()
        .entity_mut(memory_button)
        .insert(PickingInteraction::Hovered);
    app.update();
    assert_eq!(
        app.world()
            .get::<BackgroundColor>(memory_button)
            .expect("the selector keeps its background")
            .0,
        palette.hover_bg,
        "hover uses the theme hover token"
    );

    app.world_mut()
        .entity_mut(memory_button)
        .insert(PickingInteraction::Pressed);
    app.update();
    assert_eq!(
        app.world()
            .get::<BackgroundColor>(memory_button)
            .expect("the selector keeps its background")
            .0,
        palette.selection_bg,
        "pressed uses the stronger theme selection token"
    );

    app.world_mut()
        .entity_mut(memory_button)
        .insert(PickingInteraction::None);
    app.update();
    assert_eq!(
        app.world()
            .get::<BackgroundColor>(memory_button)
            .expect("the selector keeps its background")
            .0,
        palette.content_bg,
        "an idle unselected selector returns to the content surface"
    );
}

#[test]
fn folded_projection_rewrites_the_mounted_page() {
    let mut app = headless_perf_app();
    app.update();
    route_to_performance(&mut app);

    // Cold mount: the CPU summary honestly renders the shared dash.
    let cpu_field = DynField::Summary(SummaryField::Cpu);
    assert_eq!(
        dyn_text_value(app.world_mut(), &cpu_field).as_deref(),
        Some(MISSING_VALUE),
        "before any fold the summary is the honest dash"
    );

    // Fold CPU + memory + two NICs, then fire the drain trigger: the
    // observer must rewrite the marked texts and spawn the device blocks.
    fold_and_trigger(
        &mut app,
        host_batch(
            1,
            42.5,
            vec![
                nic("eth0", "eth0", 1_100, 1024, 512),
                nic("wlan0", "wlan0", 1_100, 2048, 1_024),
            ],
        ),
    );
    assert_eq!(
        dyn_text_value(app.world_mut(), &cpu_field).as_deref(),
        Some("42.5%"),
        "the folded CPU fact reached the mounted text through the observer"
    );
    assert_eq!(
        dyn_text_value(
            app.world_mut(),
            &DynField::Summary(SummaryField::NetReceive)
        )
        .as_deref(),
        Some("3.0 KiB/s"),
        "the summed receive rate of both adapters rendered"
    );
    assert_eq!(
        block_keys(app.world_mut(), Section::Network),
        vec!["eth0".to_owned(), "wlan0".to_owned()],
        "one block per projected adapter, keyed by the stable device id"
    );
    assert_eq!(
        block_keys(app.world_mut(), Section::MemorySegments).len(),
        2,
        "the composition rows followed the shell's segment list"
    );
    // The curve samples come from the HISTORY ingest, a different seam than
    // the summary's projection: one fold left the strip still collecting.
    fn strip_segments(app: &mut App) -> usize {
        let world = app.world_mut();
        let mut strips = world.query::<(&SparkStrip, &bevy::ecs::hierarchy::Children)>();
        strips
            .iter(world)
            .find(|(marker, _)| marker.0 == SystemCurve::Cpu)
            .and_then(|(_, children)| {
                children
                    .iter()
                    .next()
                    .and_then(|polyline| world.get::<bevy::ecs::hierarchy::Children>(*polyline))
            })
            .map(|segments| segments.len())
            .unwrap_or(0)
    }
    assert_eq!(
        strip_segments(&mut app),
        0,
        "one sample is still the collecting window: no segments yet"
    );
    // Two more folds warm the window; the observer must rebuild the strip's
    // polyline to follow the sample count (one segment per adjacent pair).
    fold_and_trigger(
        &mut app,
        host_batch(2, 43.0, vec![nic("eth0", "eth0", 1_200, 1024, 512)]),
    );
    fold_and_trigger(
        &mut app,
        host_batch(3, 44.0, vec![nic("eth0", "eth0", 1_300, 1024, 512)]),
    );
    assert_eq!(
        strip_segments(&mut app),
        2,
        "one connecting segment per adjacent sample pair"
    );
}

#[test]
fn device_blocks_follow_the_projection_device_list() {
    let mut app = headless_perf_app();
    app.update();
    route_to_performance(&mut app);

    fold_and_trigger(
        &mut app,
        host_batch(1, 20.0, vec![nic("eth0", "eth0", 1_100, 512, 256)]),
    );
    assert_eq!(
        block_keys(app.world_mut(), Section::Network),
        vec!["eth0".to_owned()]
    );

    // A second fold where the adapter list changed: the old block despawns
    // and the new one spawns — the projection is the only device memory.
    fold_and_trigger(
        &mut app,
        host_batch(2, 21.0, vec![nic("enp5s0", "enp5s0", 1_200, 1_024, 512)]),
    );
    assert_eq!(
        block_keys(app.world_mut(), Section::Network),
        vec!["enp5s0".to_owned()],
        "a vanished device's block vanished with it; no page-side cache"
    );
    // The new block's joined fact line rendered from the live projection
    // (receive first; unset facts stay dashes inside the line).
    let field = DynField::Device {
        section: Section::Network,
        device: "enp5s0".to_owned(),
    };
    let line = dyn_text_value(app.world_mut(), &field).expect("the new block's fact line");
    assert!(
        line.starts_with("1.0 KiB/s"),
        "the block's receive rate leads the fact line: {line}"
    );

    // Routing away unmounts the page: no markers remain for a later fold to
    // touch (idle/unmounted frames do zero work).
    app.world_mut().resource_mut::<Route>().page = Page::Processes;
    app.world_mut()
        .commands()
        .trigger(RouteChanged(Page::Processes));
    app.update();
    app.update();
    let world = app.world_mut();
    let mut texts = world.query::<&DynText>();
    assert_eq!(
        texts.iter(world).count(),
        0,
        "unmounting despawns every dynamic marker"
    );
    let mut blocks = world.query::<&DynBlock>();
    assert_eq!(blocks.iter(world).count(), 0);
}

#[test]
fn gpu_curve_card_is_gated_on_gpu_data_existence() {
    // The GPU card is display-gated: absent until GPU facts exist, so a host
    // without GPU telemetry never shows an empty fourth card.
    let mut app = headless_perf_app();
    app.update();
    route_to_performance(&mut app);

    fn gpu_card_display(app: &mut App) -> bevy::ui::Display {
        let world = app.world_mut();
        let mut gates = world.query::<(&CurveGate, &bevy::ui::Node)>();
        gates
            .iter(world)
            .find(|(gate, _)| gate.0 == SystemCurve::Gpu)
            .map(|(_, node)| node.display)
            .expect("the GPU card root is mounted with its gate")
    }
    assert_eq!(
        gpu_card_display(&mut app),
        bevy::ui::Display::None,
        "a host with no GPU facts shows no empty fourth card"
    );

    let cpu_card_display = {
        let world = app.world_mut();
        let mut gates = world.query::<(&CurveGate, &bevy::ui::Node)>();
        gates
            .iter(world)
            .find(|(gate, _)| gate.0 == SystemCurve::Cpu)
            .map(|(_, node)| node.display)
            .expect("the CPU card root is mounted with its gate")
    };
    assert_eq!(
        cpu_card_display,
        bevy::ui::Display::Flex,
        "the non-GPU cards are not accidentally closed by the GPU gate"
    );

    // CPU/network/memory data is not GPU data: the gate stays closed.
    fold_and_trigger(
        &mut app,
        host_batch(1, 20.0, vec![nic("eth0", "eth0", 1_100, 512, 256)]),
    );
    assert_eq!(
        gpu_card_display(&mut app),
        bevy::ui::Display::None,
        "non-GPU facts must not open the GPU card gate"
    );
}

// ---- memory composition bar: pure layout math -----------------------------

#[test]
fn composition_bar_fractions_sum_to_one_and_zero_total_is_empty() {
    use crate::pages::performance::scene::blocks::segment_bar_layout;
    use taskmanager_shell::memory::memory_segments;

    let memory = memory_metrics(1, 4 * GIB, 16 * GIB, 12 * GIB, (GIB, 4 * GIB));
    let segments = memory_segments(&memory);
    let layout = segment_bar_layout(&segments);
    assert_eq!(layout.len(), segments.len(), "one span per segment");
    let total: f32 = layout.iter().map(|span| span.fraction).sum();
    assert!(
        (total - 1.0).abs() < 1e-4,
        "the spans tile the full width: {total}"
    );
    for span in &layout {
        assert!(
            span.fraction.is_finite() && span.fraction >= 0.0,
            "a span width is a real share, never NaN"
        );
    }

    // Nothing measured yet: an empty layout, never NaN widths.
    let zero = taskmanager_core::core::metrics::MemoryMetrics::default();
    assert!(segment_bar_layout(&memory_segments(&zero)).is_empty());
}
