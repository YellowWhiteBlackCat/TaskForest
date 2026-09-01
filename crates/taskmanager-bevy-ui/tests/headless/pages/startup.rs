//! test-intent: behavior
//!
//! Headless behavior tests for the Startup page (same two layers as the
//! services page tests): the pure row view model projects through the shared
//! startup sort with source/impact evidence formatting and an honest
//! boot-evidence line, the id-keyed selection survives re-sorts, and the
//! wired `MinimalPlugins` page renders folded rows, repaints only on
//! startup-domain folds, routes header clicks through the shell's sort entry
//! and resolves clicked/moved rows ONLY through `sorted_startup_entry_at`.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;
use bevy::ui::BackgroundColor;
use taskmanager_application::i18n::t;
use taskmanager_application::{
    CorrelatedStartupEvent, HostTelemetryRequest, PlatformClient, PlatformEvent,
    PlatformEventBatch, PlatformFacets, PlatformHandle, ProjectedStartupEvidence, StartupEvent,
    StartupEvidenceRevision, StartupEvidenceUnavailable, SystemFacets,
};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::ProviderId;
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_core::core::startup::{
    StartupBootEvidenceSnapshot, StartupControlPolicy, StartupEntry, StartupEntryId,
    StartupEntryLocator, StartupImpact, StartupImpactEvidence, StartupImpactUnknownReason,
    StartupScope, StartupSource,
};
use taskmanager_platform_contract::{
    CapabilityCatalog, CapabilityDescriptor, CapabilityId, CapabilitySnapshot, CapabilityStatus,
    EventEnvelope, EventPort, EventPortError, EventSequence, PartialSourceSnapshot,
    RequestEnvelope, RequestId, RequestPort, SubmissionError,
};

use taskmanager_shell::{InfoSortCol, InfoTable, ShellApp, SortDir};
use taskmanager_theme::Theme;

use super::{
    EnabledChip, StartupRowClicked, StartupRowMarker, StartupSelection, StartupSelectionMoved,
    StartupSortClicked, StartupStatusLine, chip_fill, empty_state_text, enabled_chip,
    evidence_line, moved_row, selected_row, startup_impact_text, startup_rows, startup_source_text,
    status_line_text,
};
use crate::app::{FrontendTrack, Page, Route, RouteChanged};
use crate::palette::ui_palette;
use crate::runtime::{RuntimeCache, SharedRuntime};
use crate::window::FrontendWindowPlugin;
use crate::window::tests::HeadlessFrontendPlugins;

// ---- fixtures ----

fn startup_entry(id: &str, name: &str, enabled: bool) -> StartupEntry {
    StartupEntry {
        id: StartupEntryId::new(id),
        name: name.to_owned(),
        exec: format!("/usr/bin/{name}"),
        enabled,
        source: StartupSource::UserService,
        scope: StartupScope::User,
        control_policy: StartupControlPolicy::Direct,
        locator: StartupEntryLocator::new(format!("user/{id}")),
        impact: StartupImpact::Low,
        impact_evidence: StartupImpactEvidence::Unknown {
            reason: StartupImpactUnknownReason::NotInstrumented,
        },
    }
}

fn startup_batch(entries: Vec<StartupEntry>) -> PlatformEventBatch {
    PlatformEventBatch {
        startup_events: vec![CorrelatedStartupEvent {
            request_id: RequestId::MIN,
            capability: CapabilityId::STARTUP,
            provider: None,
            sequence: EventSequence::new(1),
            observed_at_ms: 1,
            event: StartupEvent::Snapshot(PartialSourceSnapshot {
                items: entries,
                sources: Vec::new(),
            }),
        }],
        ..PlatformEventBatch::default()
    }
}

fn failed_source() -> Vec<SourceStatus> {
    vec![SourceStatus {
        provider: ProviderId::borrowed("test.startup"),
        outcome: SourceOutcome::Unavailable(FailureKind::TimedOut),
        item_count: 0,
    }]
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

fn descriptor(id: CapabilityId, status: CapabilityStatus) -> CapabilityDescriptor {
    CapabilityDescriptor {
        id,
        status,
        providers: Vec::new(),
        observed_at_ms: 1,
        last_success_at_ms: None,
    }
}

fn headless_startup_app() -> (App, Arc<ScriptedEvents>) {
    let events = Arc::new(ScriptedEvents::default());
    let snapshot = CapabilitySnapshot::from_descriptors([descriptor(
        CapabilityId::STARTUP,
        CapabilityStatus::Available,
    )]);
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
        palette: ui_palette(&Theme::dark()),
    });
    app.init_resource::<bevy::asset::Assets<bevy::text::Font>>();
    (app, events)
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

/// Route before the first update: the app mounts the Startup page on frame 1
/// and never mounts the Processes default route (out of this page's scope).
fn route_to_startup(app: &mut App) {
    app.world_mut().resource_mut::<Route>().page = Page::Startup;
    app.world_mut().commands().trigger(RouteChanged);
}

fn row_targets(app: &mut App) -> Vec<(usize, String)> {
    app.world_mut()
        .query_filtered::<&StartupRowMarker, ()>()
        .iter(app.world())
        .map(|marker| (marker.0, marker.1.as_str().to_owned()))
        .collect()
}

fn row_entities(app: &mut App) -> Vec<(Entity, String)> {
    app.world_mut()
        .query_filtered::<(Entity, &StartupRowMarker), ()>()
        .iter(app.world())
        .map(|(entity, marker)| (entity, marker.1.as_str().to_owned()))
        .collect()
}

fn status_line(app: &mut App) -> String {
    app.world_mut()
        .query_filtered::<&bevy::ui::widget::Text, With<StartupStatusLine>>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .next()
        .unwrap_or_default()
}

fn selected_row_target(app: &mut App) -> Option<String> {
    let palette = ui_palette(&Theme::dark());
    let highlight = palette.nav_active_bg.to_srgba();
    app.world_mut()
        .query_filtered::<(&StartupRowMarker, &BackgroundColor), ()>()
        .iter(app.world())
        .find(|(_, fill)| fill.0.to_srgba() == highlight)
        .map(|(marker, _)| marker.1.as_str().to_owned())
}

// ---- pure: row model, formatting, chips, copy, selection ----

#[test]
fn rows_carry_source_scope_and_honest_impact_evidence() {
    let mut measured = startup_entry("ent-a", "alpha", true);
    measured.impact_evidence = StartupImpactEvidence::Measured { duration_ms: 42 };
    assert_eq!(
        startup_source_text(&measured),
        format!(
            "{} · {}",
            StartupSource::UserService.as_str(),
            t("startup.scope_user")
        ),
        "the source column carries its scope suffix (GPUI parity)"
    );
    assert_eq!(
        startup_impact_text(&measured),
        format!("{} · 42 ms", t("startup.impact_low")),
        "a measured impact carries its duration"
    );
    let unmeasured = startup_entry("ent-b", "beta", false);
    assert_eq!(
        startup_impact_text(&unmeasured),
        format!(
            "{} · {}",
            t("startup.impact_low"),
            t("startup.impact_unmeasured")
        ),
        "an unmeasured impact says so — never a fabricated duration"
    );
}

#[test]
fn rows_project_through_the_shared_sort_with_enabled_first() {
    let mut shell = ShellApp::new();
    shell.apply_platform_batch(startup_batch(vec![
        startup_entry("ent-z", "zeta", false),
        startup_entry("ent-a", "alpha", true),
        startup_entry("ent-m", "mike", true),
    ]));
    let provider_order: Vec<String> = startup_rows(&shell)
        .into_iter()
        .map(|row| row.target.as_str().to_owned())
        .collect();
    assert_eq!(provider_order, ["ent-z", "ent-a", "ent-m"]);
    // The shared Status sort ranks enabled entries first under ascending.
    shell.set_info_sort(InfoTable::Startup, InfoSortCol::Status);
    let enabled_first: Vec<String> = startup_rows(&shell)
        .into_iter()
        .map(|row| row.target.as_str().to_owned())
        .collect();
    assert_eq!(enabled_first, ["ent-a", "ent-m", "ent-z"]);
    shell.set_info_sort(InfoTable::Startup, InfoSortCol::Name);
    let by_name: Vec<String> = startup_rows(&shell)
        .into_iter()
        .map(|row| row.target.as_str().to_owned())
        .collect();
    assert_eq!(by_name, ["ent-a", "ent-m", "ent-z"]);
    shell.set_info_sort(InfoTable::Startup, InfoSortCol::Name);
    let by_name_desc: Vec<String> = startup_rows(&shell)
        .into_iter()
        .map(|row| row.target.as_str().to_owned())
        .collect();
    assert_eq!(by_name_desc, ["ent-z", "ent-m", "ent-a"]);
}

#[test]
fn selection_is_id_keyed_and_survives_a_sort_flip() {
    let mut shell = ShellApp::new();
    shell.apply_platform_batch(startup_batch(vec![
        startup_entry("ent-b", "beta", true),
        startup_entry("ent-a", "alpha", true),
    ]));
    let selection = StartupSelection {
        target: Some(StartupEntryId::new("ent-a")),
    };
    let rows = startup_rows(&shell);
    assert_eq!(selected_row(&rows, &selection), Some(1));
    shell.set_info_sort(InfoTable::Startup, InfoSortCol::Name);
    let rows = startup_rows(&shell);
    assert_eq!(
        selection.target.as_ref().map(StartupEntryId::as_str),
        Some("ent-a"),
        "the target id never drifts"
    );
    assert_eq!(selected_row(&rows, &selection), Some(0));
    let gone = StartupSelection {
        target: Some(StartupEntryId::new("ent-gone")),
    };
    assert_eq!(selected_row(&rows, &gone), None);
}

#[test]
fn cursor_moves_clamp_at_the_table_bounds() {
    assert_eq!(moved_row(2, Some(0), -1), Some(0));
    assert_eq!(moved_row(2, Some(1), 1), Some(1));
    assert_eq!(moved_row(2, None, 1), Some(0), "enters at the first row");
    assert_eq!(moved_row(0, Some(0), 1), None, "an empty table has no rows");
}

#[test]
fn enabled_chips_map_to_distinct_token_tints() {
    let palette = ui_palette(&Theme::dark());
    assert_eq!(enabled_chip(true), EnabledChip::Positive);
    assert_eq!(enabled_chip(false), EnabledChip::Idle);
    let positive = chip_fill(EnabledChip::Positive, &palette).to_srgba();
    let idle = chip_fill(EnabledChip::Idle, &palette).to_srgba();
    assert_ne!(positive, idle);
    let accent = palette.accent.to_srgba();
    assert_eq!(
        (positive.red, positive.green, positive.blue),
        (accent.red, accent.green, accent.blue),
        "chips stay tints of palette tokens"
    );
}

#[test]
fn empty_state_copy_separates_confirmed_empty_from_failed_source() {
    assert_eq!(empty_state_text(None), t("empty.no_startup_reported"));
    assert_eq!(empty_state_text(Some(&[])), t("empty.no_startup_reported"));
    let failed = empty_state_text(Some(&failed_source()));
    assert!(
        failed.contains(t("source.unavailable_title")),
        "a typed failure names itself: {failed}"
    );
}

#[test]
fn evidence_line_stays_silent_then_honest() {
    let shell = ShellApp::new();
    assert_eq!(
        evidence_line(&shell),
        None,
        "no observation yet renders nothing, not a fabricated zero"
    );
    let mut shell = ShellApp::new();
    shell.apply_platform_batch(PlatformEventBatch {
        startup_evidence_projections: vec![ProjectedStartupEvidence {
            revision: StartupEvidenceRevision::new(1),
            snapshot: StartupBootEvidenceSnapshot::default(),
            unavailable: Some(StartupEvidenceUnavailable::Provider(FailureKind::TimedOut)),
        }],
        ..PlatformEventBatch::default()
    });
    assert_eq!(
        evidence_line(&shell).as_deref(),
        Some(t("startup.evidence_unavailable")),
        "the typed unavailable marker renders instead of stale segments"
    );
    let mut shell = ShellApp::new();
    let mut snapshot = StartupBootEvidenceSnapshot::default();
    snapshot
        .critical_chain
        .push(taskmanager_core::core::startup::StartupCriticalChainNode {
            unit: "multi-user.target".to_owned(),
            activated_at_ms: Some(1200),
            duration_ms: Some(300),
        });
    snapshot
        .failed_units
        .push(taskmanager_core::core::startup::StartupFailedUnit {
            unit: "broken.service".to_owned(),
            load_state: "loaded".to_owned(),
            active_state: "failed".to_owned(),
            sub_state: "failed".to_owned(),
            description: "broken".to_owned(),
        });
    shell.apply_platform_batch(PlatformEventBatch {
        startup_evidence_projections: vec![ProjectedStartupEvidence {
            revision: StartupEvidenceRevision::new(2),
            snapshot,
            unavailable: None,
        }],
        ..PlatformEventBatch::default()
    });
    let line = evidence_line(&shell).unwrap_or_default();
    assert!(
        line.contains("1") && line.contains(t("startup.critical_chain")),
        "the chain summary counts its nodes: {line}"
    );
    assert!(
        line.contains(t("startup.failed_units")),
        "failed units stay visible: {line}"
    );
    assert_eq!(
        status_line_text(&shell, 0),
        format!("0 {} · provider order", t("startup.noun")),
        "the summary line counts rows with the shared noun"
    );
}

// ---- wired: fold → rows, sort click, selection, idle ----

#[test]
fn folded_rows_render_then_refresh_and_idle_frames_redraw_nothing() {
    let (mut app, events) = headless_startup_app();
    route_to_startup(&mut app);
    push_startup(
        &events,
        vec![
            startup_entry("ent-b", "beta", true),
            startup_entry("ent-a", "alpha", false),
        ],
    );
    app.update();
    app.update();
    assert_eq!(
        row_targets(&mut app),
        [(0, "ent-b".to_owned()), (1, "ent-a".to_owned())],
        "rows render in provider order until a sort is picked"
    );
    assert_eq!(
        status_line(&mut app),
        format!("2 {} · provider order", t("startup.noun"))
    );

    let before = row_entities(&mut app);
    app.update();
    app.update();
    assert_eq!(
        before,
        row_entities(&mut app),
        "no fold, no repaint — idle frames redraw nothing"
    );

    push_startup(
        &events,
        vec![
            startup_entry("ent-z", "zulu", true),
            startup_entry("ent-a", "alpha", true),
            startup_entry("ent-m", "mike", false),
        ],
    );
    app.update();
    app.update();
    assert_eq!(
        row_targets(&mut app),
        [
            (0, "ent-z".to_owned()),
            (1, "ent-a".to_owned()),
            (2, "ent-m".to_owned()),
        ],
        "the fold observer repainted the body from the new projection"
    );
}

#[test]
fn sort_click_projects_shared_order_and_keeps_selection_on_target() {
    let (mut app, events) = headless_startup_app();
    route_to_startup(&mut app);
    push_startup(
        &events,
        vec![
            startup_entry("ent-b", "beta", true),
            startup_entry("ent-a", "alpha", true),
            startup_entry("ent-c", "gamma", true),
        ],
    );
    app.update();
    app.update();

    app.world_mut().trigger(StartupRowClicked(1));
    app.update();
    assert_eq!(selected_row_target(&mut app).as_deref(), Some("ent-a"));

    app.world_mut()
        .trigger(StartupSortClicked(InfoSortCol::Name));
    app.update();
    app.update();
    assert_eq!(
        app.world().non_send::<FrontendTrack>().shell.startup_sort,
        Some((InfoSortCol::Name, SortDir::Asc)),
        "the observer routed the click through the shell's sort entry"
    );
    assert_eq!(
        row_targets(&mut app),
        [
            (0, "ent-a".to_owned()),
            (1, "ent-b".to_owned()),
            (2, "ent-c".to_owned()),
        ]
    );
    assert_eq!(
        selected_row_target(&mut app).as_deref(),
        Some("ent-a"),
        "the selected target id survived the reorder"
    );
}

#[test]
fn keyboard_moves_clamp_and_out_of_range_clicks_change_nothing() {
    let (mut app, events) = headless_startup_app();
    route_to_startup(&mut app);
    push_startup(
        &events,
        vec![
            startup_entry("ent-b", "beta", true),
            startup_entry("ent-a", "alpha", true),
        ],
    );
    app.update();
    app.update();
    app.world_mut()
        .trigger(StartupSortClicked(InfoSortCol::Name));
    app.update();

    app.world_mut().trigger(StartupSelectionMoved(1));
    app.update();
    assert_eq!(selected_row_target(&mut app).as_deref(), Some("ent-a"));
    app.world_mut().trigger(StartupSelectionMoved(-9));
    app.update();
    assert_eq!(
        selected_row_target(&mut app).as_deref(),
        Some("ent-a"),
        "the cursor saturates at the first row"
    );
    app.world_mut().trigger(StartupSelectionMoved(9));
    app.update();
    assert_eq!(
        selected_row_target(&mut app).as_deref(),
        Some("ent-b"),
        "the cursor saturates at the last row"
    );
    app.world_mut().trigger(StartupRowClicked(7));
    app.update();
    assert_eq!(
        selected_row_target(&mut app).as_deref(),
        Some("ent-b"),
        "an out-of-range click leaves the selection untouched"
    );
    assert_eq!(
        app.world().resource::<StartupSelection>().target,
        Some(StartupEntryId::new("ent-b")),
        "the selection resource stays id-keyed and honest"
    );
}
