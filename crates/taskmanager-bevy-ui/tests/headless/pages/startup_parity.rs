//! test-intent: behavior
//!
//! Headless behavior tests for the Startup page's boot-visibility surfaces:
//! the three-tier impact badge (BEVY-145), the placed boot-timeline gantt
//! (BEVY-146), the pinned failed-boot forensics panel with its error-detail
//! accordion (BEVY-147) and the boot-phase proportion rows (BEVY-148).
//!
//! Every assertion reads the RENDERED scene — the badge tiers and their
//! palette inks, the gantt windows' placed geometry, the panel's position in
//! the page tree and the accordion's open set — so a change that only
//! rewrites the projection (and not the page) fails these tests.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::query::With;
use bevy::text::TextColor;
use bevy::ui::widget::Text;
use bevy::ui::{BackgroundColor, Node};
use taskmanager_application::i18n::t;
use taskmanager_application::{
    HostTelemetryRequest, PlatformClient, PlatformEvent, PlatformFacets, PlatformHandle,
    StartupEvent, SystemFacets,
};
use taskmanager_core::core::startup::{
    StartupBootEvidenceSnapshot, StartupControlPolicy, StartupCriticalChainNode, StartupEntry,
    StartupEntryId, StartupEntryLocator, StartupFailedUnit, StartupImpact, StartupImpactEvidence,
    StartupImpactUnknownReason, StartupScope, StartupSource, StartupTimelineSnapshot,
};
use taskmanager_platform_contract::{
    CapabilityCatalog, CapabilityDescriptor, CapabilityId, CapabilitySnapshot, CapabilityStatus,
    EventEnvelope, EventPort, EventPortError, EventSequence, PartialSourceSnapshot,
    RequestEnvelope, RequestId, RequestPort, SubmissionError,
};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::UiSize;

use super::forensics::{
    StartupForensicsDetail, StartupForensicsPanel, StartupForensicsToggle, StartupForensicsUnit,
};
use super::impact::{ImpactTier, StartupImpactBadge, StartupImpactMark};
use super::phases::{BootPhase, StartupPhaseBar, StartupPhaseRow};
use super::timeline::{
    OVERVIEW_WINDOW_ROWS, StartupTimelineCollapsed, StartupTimelineTotal, StartupTimelineUntimed,
    StartupTimelineWindow,
};
use super::{StartupBody, StartupSelection};
use crate::app::{FrontendTrack, Page, Route, RouteChanged};
use crate::drain::ShellProjectionFolded;
use crate::palette::ui_palette;
use crate::runtime::{RuntimeCache, SharedRuntime};
use crate::window::FrontendWindowPlugin;
use crate::window::tests::HeadlessFrontendPlugins;
use taskmanager_shell::fixture::ProjectionSeedFact;
use taskmanager_shell::fixture::seed_projection_fact;

// ---- fixtures ----

fn startup_entry(id: &str, name: &str, impact: StartupImpact) -> StartupEntry {
    StartupEntry {
        id: StartupEntryId::new(id),
        name: name.to_owned(),
        exec: format!("/usr/bin/{name}"),
        enabled: true,
        source: StartupSource::UserService,
        scope: StartupScope::User,
        control_policy: StartupControlPolicy::Direct,
        locator: StartupEntryLocator::new(format!("user/{id}")),
        impact,
        impact_evidence: StartupImpactEvidence::Unknown {
            reason: StartupImpactUnknownReason::NotInstrumented,
        },
    }
}

struct FixedCapabilities(CapabilitySnapshot);

impl CapabilityCatalog for FixedCapabilities {
    fn snapshot(&self) -> CapabilitySnapshot {
        self.0.clone()
    }
}

#[derive(Default)]
struct ScriptedEvents(Mutex<VecDeque<EventEnvelope<PlatformEvent>>>);

impl EventPort for ScriptedEvents {
    type Event = PlatformEvent;

    fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError> {
        Ok(self.0.lock().expect("scripted port lock").pop_front())
    }
}

struct QuietRequests;

impl RequestPort for QuietRequests {
    type Request = HostTelemetryRequest;

    fn try_submit(&self, _request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        Ok(())
    }
}

fn headless_startup_app() -> (App, Arc<ScriptedEvents>) {
    let events = Arc::new(ScriptedEvents::default());
    let snapshot = CapabilitySnapshot::from_descriptors([
        descriptor(CapabilityId::STARTUP, CapabilityStatus::Available),
        descriptor(CapabilityId::STARTUP_EVIDENCE, CapabilityStatus::Available),
    ]);
    let port = events.clone();
    let client = PlatformClient::new(PlatformHandle::new(
        Arc::new(FixedCapabilities(snapshot)),
        port,
        PlatformFacets::default()
            .with_system(SystemFacets::default().with_host(Arc::new(QuietRequests))),
    ));
    let cache: &'static RuntimeCache = Box::leak(Box::new(RuntimeCache::new()));
    let runtime: &'static SharedRuntime = cache
        .get_or_init(move || Ok(client))
        .expect("scripted runtime starts");
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(HeadlessFrontendPlugins);
    app.add_plugins(FrontendWindowPlugin {
        runtime,
        palette: ui_palette(&Theme::dark(), UiSize::Standard),
    });
    app.init_resource::<bevy::asset::Assets<bevy::text::Font>>();
    (app, events)
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

fn route_to_startup(app: &mut App) {
    app.world_mut().resource_mut::<Route>().page = Page::Startup;
    app.world_mut().commands().trigger(RouteChanged);
}

fn push_startup(events: &ScriptedEvents, entries: Vec<StartupEntry>) {
    events
        .0
        .lock()
        .expect("scripted port lock")
        .push_back(EventEnvelope {
            request_id: RequestId::MIN,
            capability: CapabilityId::STARTUP,
            provider: None,
            sequence: EventSequence::new(1),
            observed_at_ms: 1,
            outcome: Ok(PlatformEvent::Startup(StartupEvent::Snapshot(
                PartialSourceSnapshot {
                    items: entries,
                    sources: Vec::new(),
                },
            ))),
        });
}

/// Seed the boot evidence into the live shell projection and raise the
/// page's data-refresh event.
///
/// The evidence is seeded through the shell's OWN public fixture reducer —
/// the same seam the other frontends' headless tests use — so the page
/// reads the shared projection exactly as it would after a real fold. The
/// refresh event is the one the drain raises after folding a batch.
fn seed_boot_evidence(app: &mut App, snapshot: StartupBootEvidenceSnapshot) {
    {
        let mut track = app.world_mut().non_send_mut::<FrontendTrack>();
        seed_projection_fact(
            &mut track.shell,
            ProjectionSeedFact::StartupBootEvidence(Some(snapshot)),
        );
    }
    app.world_mut().commands().trigger(ShellProjectionFolded);
}

fn chain_node(
    unit: &str,
    activated_at_ms: Option<u64>,
    duration_ms: Option<u64>,
) -> StartupCriticalChainNode {
    StartupCriticalChainNode {
        unit: unit.to_owned(),
        activated_at_ms,
        duration_ms,
    }
}

fn failed_unit(unit: &str) -> StartupFailedUnit {
    StartupFailedUnit {
        unit: unit.to_owned(),
        load_state: "loaded".to_owned(),
        active_state: "failed".to_owned(),
        sub_state: "failed".to_owned(),
        description: format!("{unit} exited with a non-zero status"),
    }
}

/// The scene root the page paints into: the Startup body's only child.
fn body_root(app: &mut App) -> Entity {
    let mut roots: Vec<(Entity, Entity)> = app
        .world_mut()
        .query_filtered::<(Entity, &Children), With<StartupBody>>()
        .iter(app.world())
        .map(|(entity, children)| (entity, *children.first().expect("painted body")))
        .collect();
    assert_eq!(roots.len(), 1, "the Startup page mounts exactly one body");
    roots.remove(0).1
}

/// The direct children of the painted scene root, in scene order.
fn section_order(app: &mut App) -> Vec<Entity> {
    let root = body_root(app);
    app.world()
        .entity(root)
        .get::<Children>()
        .expect("scene root children")
        .iter()
        .copied()
        .collect()
}

// ---- BEVY-145: the three-tier impact badge ----

#[test]
fn impact_badge_shows_three_tier_colors_from_palette_tokens() {
    let (mut app, events) = headless_startup_app();
    route_to_startup(&mut app);
    push_startup(
        &events,
        vec![
            startup_entry("ent-high", "alpha", StartupImpact::High),
            startup_entry("ent-med", "beta", StartupImpact::Medium),
            startup_entry("ent-low", "gamma", StartupImpact::Low),
        ],
    );
    app.update();
    app.update();

    let badges: Vec<(Entity, ImpactTier)> = app
        .world_mut()
        .query_filtered::<(Entity, &StartupImpactBadge), ()>()
        .iter(app.world())
        .map(|(entity, badge)| (entity, badge.0))
        .collect();
    assert_eq!(
        badges.len(),
        3,
        "every startup row carries one impact badge"
    );

    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let expected = [
        (ImpactTier::High, palette.danger_color.to_srgba()),
        (ImpactTier::Medium, palette.warning_color.to_srgba()),
        (ImpactTier::Low, palette.cpu.to_srgba()),
    ];
    for (tier, ink) in expected {
        assert!(
            badges.iter().any(|(_, badge_tier)| *badge_tier == tier),
            "a {tier:?} badge exists"
        );
        let mark = app
            .world_mut()
            .query_filtered::<(&StartupImpactMark, &BackgroundColor), ()>()
            .iter(app.world())
            .find(|(mark, _)| mark.0 == tier)
            .map(|(_, fill)| fill.0.to_srgba())
            .expect("the badge's tier column");
        assert_eq!(
            mark, ink,
            "the {tier:?} tier column wears the palette's own ink"
        );
        let label_ink = app
            .world_mut()
            .query_filtered::<(&StartupImpactBadge, &TextColor), ()>()
            .iter(app.world())
            .find(|(badge, _)| badge.0 == tier)
            .map(|(_, ink)| ink.0.to_srgba());
        assert_eq!(
            label_ink,
            Some(ink),
            "the {tier:?} badge label wears the tier's ink"
        );
    }

    let inks = app
        .world_mut()
        .query_filtered::<(&StartupImpactMark, &BackgroundColor), ()>()
        .iter(app.world())
        .map(|(_, fill)| fill.0.to_srgba())
        .collect::<Vec<_>>();
    assert_eq!(inks.len(), 3, "the three tiers own one column each");
    assert!(
        inks[0] != inks[1] && inks[1] != inks[2] && inks[0] != inks[2],
        "the three tiers are three distinct colors: {inks:?}"
    );
}

#[test]
fn impact_badge_keeps_the_shared_impact_evidence_in_its_label() {
    let (mut app, events) = headless_startup_app();
    route_to_startup(&mut app);
    let mut entry = startup_entry("ent-high", "alpha", StartupImpact::High);
    entry.impact_evidence = StartupImpactEvidence::Measured { duration_ms: 42 };
    push_startup(&events, vec![entry]);
    app.update();
    app.update();

    let labels: Vec<String> = app
        .world_mut()
        .query_filtered::<&Text, ()>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .collect();
    assert!(
        labels.iter().any(|label| label.contains("42 ms")),
        "the badge keeps the measured duration: {labels:?}"
    );
    assert!(
        labels
            .iter()
            .any(|label| label.contains(t("startup.impact_high"))),
        "the badge keeps the shared tier word: {labels:?}"
    );
}

// ---- BEVY-146: the placed boot-timeline gantt ----

#[test]
fn boot_timeline_places_every_measured_window_on_the_boot_axis() {
    let (mut app, _events) = headless_startup_app();
    route_to_startup(&mut app);
    let mut snapshot = StartupBootEvidenceSnapshot::default();
    snapshot
        .critical_chain
        .push(chain_node("alpha.service", Some(0), Some(100)));
    snapshot
        .critical_chain
        .push(chain_node("beta.service", Some(500), Some(300)));
    snapshot
        .critical_chain
        .push(chain_node("gamma.service", Some(2_000), Some(200)));
    snapshot
        .critical_chain
        .push(chain_node("untimed.service", None, None));
    seed_boot_evidence(&mut app, snapshot);
    app.update();
    app.update();

    let windows: Vec<(StartupTimelineWindow, Node)> = app
        .world_mut()
        .query_filtered::<(&StartupTimelineWindow, &Node), ()>()
        .iter(app.world())
        .map(|(window, node)| (window.clone(), node.clone()))
        .collect();
    assert_eq!(
        windows.len(),
        3,
        "every measured unit reaches the gantt: {windows:?}"
    );

    let total: u64 = app
        .world_mut()
        .query_filtered::<&StartupTimelineTotal, ()>()
        .iter(app.world())
        .next()
        .expect("the gantt total")
        .0;
    assert_eq!(total, 2_200, "the axis spans the boot's measured window");

    for (window, node) in &windows {
        let start_share = window.1 as f32 / total as f32 * 100.0;
        let duration_share = window.2 as f32 / total as f32 * 100.0;
        let margin = node.margin.left;
        let width = node.width;
        assert!(
            matches!(margin, bevy::ui::Val::Percent(value) if (value - start_share).abs() < 0.01),
            "{} is placed at its activation offset: {margin:?}",
            window.0
        );
        assert!(
            matches!(width, bevy::ui::Val::Percent(value) if (value - duration_share).abs() < 0.01),
            "{} is sized by its duration share: {width:?}",
            window.0
        );
    }

    let untimed = app
        .world_mut()
        .query_filtered::<&StartupTimelineUntimed, ()>()
        .iter(app.world())
        .next()
        .expect("the untimed row");
    assert_eq!(
        untimed.0, 1,
        "a unit without activation data is counted, never placed"
    );
}

#[test]
fn boot_timeline_stays_silent_without_measured_evidence() {
    let (mut app, events) = headless_startup_app();
    route_to_startup(&mut app);
    push_startup(
        &events,
        vec![startup_entry("ent-a", "alpha", StartupImpact::Low)],
    );
    app.update();
    app.update();

    let windows = app
        .world_mut()
        .query_filtered::<&StartupTimelineWindow, ()>()
        .iter(app.world())
        .count();
    assert_eq!(windows, 0, "no evidence means no gantt");
    let phases = app
        .world_mut()
        .query_filtered::<&StartupPhaseRow, ()>()
        .iter(app.world())
        .count();
    assert_eq!(phases, 0, "no evidence means no phase rows");
    let panels = app
        .world_mut()
        .query_filtered::<&StartupForensicsPanel, ()>()
        .iter(app.world())
        .count();
    assert_eq!(panels, 0, "no evidence means no forensics panel");
}

// ---- BEVY-147: the pinned failed-boot forensics panel ----

#[test]
fn failed_units_pin_a_forensics_panel_with_an_open_error_detail() {
    let (mut app, _events) = headless_startup_app();
    route_to_startup(&mut app);
    let mut snapshot = StartupBootEvidenceSnapshot::default();
    snapshot.failed_units.push(failed_unit("broken.service"));
    snapshot.failed_units.push(failed_unit("crashed.service"));
    seed_boot_evidence(&mut app, snapshot);
    app.update();
    app.update();

    let panels: Vec<Entity> = app
        .world_mut()
        .query_filtered::<Entity, With<StartupForensicsPanel>>()
        .iter(app.world())
        .collect();
    assert_eq!(panels.len(), 1, "a failed boot mounts one forensics panel");

    let order = section_order(&mut app);
    assert_eq!(
        order.first().copied(),
        panels.first().copied(),
        "the forensics panel is pinned to the top of the page: {order:?}"
    );

    let units: Vec<String> = app
        .world_mut()
        .query_filtered::<&StartupForensicsUnit, ()>()
        .iter(app.world())
        .map(|unit| unit.0.clone())
        .collect();
    assert_eq!(
        units,
        ["broken.service", "crashed.service"],
        "every failed unit reaches the panel"
    );

    let details: Vec<String> = app
        .world_mut()
        .query_filtered::<&StartupForensicsDetail, ()>()
        .iter(app.world())
        .map(|detail| detail.0.clone())
        .collect();
    assert_eq!(
        details,
        ["broken.service"],
        "the first reported failure opens with its error detail: {details:?}"
    );
}

#[test]
fn the_failed_unit_accordion_opens_the_error_detail_on_demand() {
    let (mut app, _events) = headless_startup_app();
    route_to_startup(&mut app);
    let mut snapshot = StartupBootEvidenceSnapshot::default();
    snapshot.failed_units.push(failed_unit("broken.service"));
    snapshot.failed_units.push(failed_unit("crashed.service"));
    seed_boot_evidence(&mut app, snapshot);
    app.update();
    app.update();

    let toggle = app
        .world_mut()
        .query_filtered::<(Entity, &StartupForensicsToggle), ()>()
        .iter(app.world())
        .find(|(_, toggle)| toggle.0 == "crashed.service")
        .map(|(entity, _)| entity)
        .expect("the second failed unit's toggle");
    app.world_mut()
        .commands()
        .trigger(bevy::ui_widgets::Activate { entity: toggle });
    app.update();
    app.update();

    let details: Vec<String> = app
        .world_mut()
        .query_filtered::<&StartupForensicsDetail, ()>()
        .iter(app.world())
        .map(|detail| detail.0.clone())
        .collect();
    assert_eq!(
        details,
        ["crashed.service"],
        "the user's choice is authoritative: {details:?}"
    );

    let selection = app.world().resource::<StartupSelection>();
    assert!(
        selection.target.is_none(),
        "the accordion does not steal the inventory selection"
    );
}

// ---- BEVY-148: the boot-phase proportion rows ----

#[test]
fn boot_overview_compares_three_phases_without_fabricating_the_missing_ones() {
    let (mut app, _events) = headless_startup_app();
    route_to_startup(&mut app);
    let mut snapshot = StartupBootEvidenceSnapshot::default();
    snapshot
        .critical_chain
        .push(chain_node("alpha.service", Some(0), Some(100)));
    snapshot
        .critical_chain
        .push(chain_node("beta.service", Some(500), Some(300)));
    // The platform measured ONE phase: the user-space span. The firmware
    // and kernel spans were never measured, so they stay absent — an
    // unmeasured phase is not a measured zero.
    snapshot.phase_timeline = Some(StartupTimelineSnapshot {
        firmware_ms: None,
        loader_ms: None,
        kernel_ms: None,
        user_space_ms: Some(800),
    });
    seed_boot_evidence(&mut app, snapshot);
    app.update();
    app.update();

    let rows: Vec<(StartupPhaseRow, Node)> = app
        .world_mut()
        .query_filtered::<(&StartupPhaseRow, &Node), ()>()
        .iter(app.world())
        .map(|(row, node)| (*row, node.clone()))
        .collect();
    assert_eq!(rows.len(), 3, "the overview compares three boot phases");

    let firmware = rows
        .iter()
        .find(|(row, _)| row.0 == BootPhase::Firmware)
        .expect("the firmware row");
    let kernel = rows
        .iter()
        .find(|(row, _)| row.0 == BootPhase::Kernel)
        .expect("the kernel row");
    let userspace = rows
        .iter()
        .find(|(row, _)| row.0 == BootPhase::Userspace)
        .expect("the userspace row");

    assert_eq!(
        firmware.0.1, None,
        "an unmeasured phase is never a measured zero"
    );
    assert_eq!(kernel.0.1, None);
    assert_eq!(
        userspace.0.1,
        Some(800),
        "the userspace phase carries the measured boot span"
    );

    let total: u64 = app
        .world_mut()
        .query_filtered::<&StartupTimelineTotal, ()>()
        .iter(app.world())
        .next()
        .expect("the boot overview total")
        .0;
    assert_eq!(total, 800, "the overview's scale is the measured boot");

    // The BAR is the measured share; the row is the label's row. An
    // unmeasured phase draws NO bar at all (not a zero-width one).
    let userspace_bar = app
        .world_mut()
        .query_filtered::<(&StartupPhaseBar, &Node), ()>()
        .iter(app.world())
        .filter(|(bar, _)| bar.0 == BootPhase::Userspace)
        .map(|(_, node)| node.width)
        .next()
        .expect("the measured phase draws its bar");
    assert!(
        matches!(userspace_bar, bevy::ui::Val::Percent(value) if (value - 100.0).abs() < 0.01),
        "the measured phase fills its share of the measured boot: {userspace_bar:?}"
    );

    let firmware_bar = app
        .world_mut()
        .query_filtered::<(&StartupPhaseBar, &Node), ()>()
        .iter(app.world())
        .filter(|(bar, _)| bar.0 == BootPhase::Firmware)
        .map(|(_, node)| node.width)
        .next();
    assert!(
        firmware_bar.is_none(),
        "an unmeasured phase draws no bar at all: {firmware_bar:?}"
    );
}

#[test]
fn a_long_chain_collapses_into_a_counted_overflow_row() {
    let (mut app, _events) = headless_startup_app();
    route_to_startup(&mut app);
    let mut snapshot = StartupBootEvidenceSnapshot::default();
    for index in 0..24_u64 {
        snapshot.critical_chain.push(chain_node(
            &format!("unit-{index}.service"),
            Some(index * 100),
            Some(50),
        ));
    }
    seed_boot_evidence(&mut app, snapshot);
    app.update();
    app.update();

    let collapsed = app
        .world_mut()
        .query_filtered::<&StartupTimelineCollapsed, ()>()
        .iter(app.world())
        .next()
        .expect("the collapsed overflow row")
        .0;
    let windows = app
        .world_mut()
        .query_filtered::<&StartupTimelineWindow, ()>()
        .iter(app.world())
        .count();
    assert_eq!(
        windows, OVERVIEW_WINDOW_ROWS,
        "the boot overview keeps its own row budget"
    );
    assert_eq!(
        collapsed,
        24 - OVERVIEW_WINDOW_ROWS,
        "every window past the budget is counted, never dropped"
    );
}
