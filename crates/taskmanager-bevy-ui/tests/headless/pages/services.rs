//! test-intent: behavior
//!
//! Headless behavior tests for the Services page.
//!
//! Two layers, mirroring `tests/headless/app.rs`:
//! - pure: the row view model projects through the shell's shared sort (never
//!   provider order), the id-keyed selection survives re-sorts, the cursor
//!   clamps at the table bounds, status chips map deterministically, and the
//!   empty/unavailable copy distinguishes a typed provider failure from a
//!   confirmed empty inventory;
//! - wired: on a `MinimalPlugins` app with the real window plugin and a
//!   scripted event port, the Services page renders folded rows, repaints on
//!   a services-domain fold, redraws nothing while the port is quiet, routes
//!   a header-sort click through the shell's sort entry, and translates
//!   clicked/moved rows to targets ONLY through `sorted_service_at`.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;
use bevy::ui::BackgroundColor;
use taskmanager_application::i18n::t;
use taskmanager_application::{
    CapabilityCatalog, CapabilityDescriptor, CapabilityId, CapabilitySnapshot, CapabilityStatus,
    CorrelatedServiceEvent, EventEnvelope, EventPort, EventPortError, EventSequence, FailureKind,
    HostTelemetryRequest, PartialSourceSnapshot, PlatformClient, PlatformEvent, PlatformFacets,
    PlatformHandle, ProviderId, RequestEnvelope, RequestId, RequestPort, ServiceEvent, ServiceId,
    ServiceItem, ServiceStatus, SourceOutcome, SourceStatus, SubmissionError, SystemFacets,
};
use taskmanager_shell::{InfoSortCol, InfoTable, ShellApp, SortDir};
use taskmanager_theme::Theme;

use super::{
    ServiceRowClicked, ServiceSelection, ServiceSelectionMoved, ServiceSortClicked,
    ServicesRowMarker, ServicesStatusLine, StatusChip, chip_fill, empty_state_text, header_label,
    moved_row, selected_row, service_chip, service_rows, status_line_text,
};
use crate::app::{FrontendTrack, Page, Route, RouteChanged};
use crate::palette::ui_palette;
use crate::runtime::{RuntimeCache, SharedRuntime};
use crate::window::FrontendWindowPlugin;
use crate::window::tests::HeadlessFrontendPlugins;

// ---- fixtures ----

fn service_item(id: &str, name: &str, status: ServiceStatus) -> ServiceItem {
    ServiceItem::from_inventory(
        id,
        name,
        status,
        format!("{name} description"),
        "loaded",
        "active",
        "running",
    )
}

fn shelved_shell(items: &[ServiceItem]) -> ShellApp {
    let mut shell = ShellApp::new();
    shell.apply_platform_batch(taskmanager_application::PlatformEventBatch {
        service_events: vec![correlated_service_snapshot(items.to_vec())],
        ..taskmanager_application::PlatformEventBatch::default()
    });
    shell
}

fn correlated_service_snapshot(items: Vec<ServiceItem>) -> CorrelatedServiceEvent {
    CorrelatedServiceEvent {
        request_id: RequestId::MIN,
        capability: CapabilityId::SERVICES,
        provider: None,
        sequence: EventSequence::new(1),
        observed_at_ms: 1,
        event: ServiceEvent::Snapshot(PartialSourceSnapshot {
            items,
            sources: Vec::new(),
        }),
    }
}

fn failed_source() -> Vec<SourceStatus> {
    vec![SourceStatus {
        provider: ProviderId::borrowed("test.services"),
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

/// Scripted event port: the test pushes service snapshots, the drain pops
/// them; an empty queue is the idle case.
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

fn headless_services_app() -> (App, Arc<ScriptedEvents>) {
    let events = Arc::new(ScriptedEvents::default());
    let snapshot = CapabilitySnapshot::from_descriptors([descriptor(
        CapabilityId::SERVICES,
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

fn push_services(events: &ScriptedEvents, items: Vec<ServiceItem>) {
    events
        .0
        .lock()
        .expect("scripted port lock")
        .push_back(EventEnvelope {
            request_id: RequestId::MIN,
            capability: CapabilityId::SERVICES,
            provider: None,
            sequence: EventSequence::new(1),
            observed_at_ms: 1,
            outcome: Ok(PlatformEvent::Services(ServiceEvent::Snapshot(
                PartialSourceSnapshot {
                    items,
                    sources: Vec::new(),
                },
            ))),
        });
}

/// Route before the first update so the app mounts the Services page on
/// frame 1 — the test never mounts the Processes default route (a sibling
/// page's in-flight work is out of scope here, and unmounted pages register
/// nothing).
fn route_to_services(app: &mut App) {
    app.world_mut().resource_mut::<Route>().page = Page::Services;
    app.world_mut()
        .commands()
        .trigger(RouteChanged(Page::Services));
}

fn row_targets(app: &mut App) -> Vec<(usize, String)> {
    app.world_mut()
        .query_filtered::<&ServicesRowMarker, ()>()
        .iter(app.world())
        .map(|marker| (marker.0, marker.1.as_str().to_owned()))
        .collect()
}

fn row_entities(app: &mut App) -> Vec<(Entity, String)> {
    app.world_mut()
        .query_filtered::<(Entity, &ServicesRowMarker), ()>()
        .iter(app.world())
        .map(|(entity, marker)| (entity, marker.1.as_str().to_owned()))
        .collect()
}

fn status_line(app: &mut App) -> String {
    app.world_mut()
        .query_filtered::<&bevy::ui::widget::Text, With<ServicesStatusLine>>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .next()
        .unwrap_or_default()
}

fn selected_row_target(app: &mut App) -> Option<String> {
    let palette = ui_palette(&Theme::dark());
    let highlight = palette.nav_active_bg.to_srgba();
    app.world_mut()
        .query_filtered::<(&ServicesRowMarker, &BackgroundColor), ()>()
        .iter(app.world())
        .find(|(_, fill)| fill.0.to_srgba() == highlight)
        .map(|(marker, _)| marker.1.as_str().to_owned())
}

fn shell_services_sort(app: &mut App) -> Option<(InfoSortCol, SortDir)> {
    app.world().non_send::<FrontendTrack>().shell.services_sort
}

// ---- pure: row model, sort projection, selection, chips, copy ----

#[test]
fn rows_project_through_the_shared_sort_never_provider_order() {
    let mut shell = shelved_shell(&[
        service_item("svc-b", "beta", ServiceStatus::Active),
        service_item("svc-a", "alpha", ServiceStatus::Failed),
        service_item("svc-c", "gamma", ServiceStatus::Inactive),
    ]);
    // No sort picked yet: provider order is the honest default.
    let provider_order: Vec<String> = service_rows(&shell)
        .into_iter()
        .map(|row| row.target.into_string())
        .collect();
    assert_eq!(provider_order, ["svc-b", "svc-a", "svc-c"]);
    // Header clicks go through the shell's sort entry; every projection
    // (rows AND the by-row target translation) follows it.
    shell.set_info_sort(InfoTable::Services, InfoSortCol::Name);
    let by_name: Vec<String> = service_rows(&shell)
        .into_iter()
        .map(|row| row.target.into_string())
        .collect();
    assert_eq!(by_name, ["svc-a", "svc-b", "svc-c"]);
    // Clicking the active column again flips the direction (shared semantics).
    shell.set_info_sort(InfoTable::Services, InfoSortCol::Name);
    let by_name_desc: Vec<String> = service_rows(&shell)
        .into_iter()
        .map(|row| row.target.into_string())
        .collect();
    assert_eq!(by_name_desc, ["svc-c", "svc-b", "svc-a"]);
    // Status sort ranks active before inactive before failed.
    shell.set_info_sort(InfoTable::Services, InfoSortCol::Status);
    let by_status: Vec<String> = service_rows(&shell)
        .into_iter()
        .map(|row| row.target.into_string())
        .collect();
    assert_eq!(by_status, ["svc-b", "svc-c", "svc-a"]);
}

#[test]
fn selection_is_id_keyed_and_survives_a_sort_flip() {
    let mut shell = shelved_shell(&[
        service_item("svc-b", "beta", ServiceStatus::Active),
        service_item("svc-a", "alpha", ServiceStatus::Active),
    ]);
    let rows = service_rows(&shell);
    let selection = ServiceSelection {
        target: Some(ServiceId::new("svc-a")),
    };
    assert_eq!(selected_row(&rows, &selection), Some(1));
    shell.set_info_sort(InfoTable::Services, InfoSortCol::Name);
    let rows = service_rows(&shell);
    // The target id is unchanged; its visual row moved with the new order.
    assert_eq!(
        selection.target.as_ref().map(ServiceId::as_str),
        Some("svc-a")
    );
    assert_eq!(selected_row(&rows, &selection), Some(0));
    // A target that left the inventory resolves to no row — never a neighbor.
    let gone = ServiceSelection {
        target: Some(ServiceId::new("svc-gone")),
    };
    assert_eq!(selected_row(&rows, &gone), None);
}

#[test]
fn cursor_moves_clamp_at_the_table_bounds() {
    assert_eq!(
        moved_row(3, Some(0), -1),
        Some(0),
        "clamps at the first row"
    );
    assert_eq!(moved_row(3, Some(2), 1), Some(2), "clamps at the last row");
    assert_eq!(moved_row(3, Some(1), -1), Some(0));
    assert_eq!(moved_row(3, None, 1), Some(0), "enters at the first row");
    assert_eq!(
        moved_row(3, Some(0), -100),
        Some(0),
        "a large delta still saturates"
    );
    assert_eq!(moved_row(0, Some(0), 1), None, "an empty table has no rows");
}

#[test]
fn status_chips_map_every_service_state_to_a_distinct_fill() {
    let palette = ui_palette(&Theme::dark());
    assert_eq!(service_chip(ServiceStatus::Active), StatusChip::Positive);
    assert_eq!(service_chip(ServiceStatus::Failed), StatusChip::Negative);
    assert_eq!(service_chip(ServiceStatus::Inactive), StatusChip::Idle);
    assert_eq!(service_chip(ServiceStatus::Unknown), StatusChip::Idle);
    let positive = chip_fill(StatusChip::Positive, &palette).to_srgba();
    let negative = chip_fill(StatusChip::Negative, &palette).to_srgba();
    let idle = chip_fill(StatusChip::Idle, &palette).to_srgba();
    assert_ne!(positive, negative, "the chip kinds stay visually distinct");
    assert_ne!(positive, idle, "the chip kinds stay visually distinct");
    assert_ne!(negative, idle, "the chip kinds stay visually distinct");
    // Chips are tints of palette tokens: same channels, reduced alpha.
    let accent = palette.accent.to_srgba();
    assert_eq!(
        (positive.red, positive.green, positive.blue),
        (accent.red, accent.green, accent.blue)
    );
    assert!(positive.alpha < accent.alpha);
}

#[test]
fn empty_state_copy_separates_confirmed_empty_from_failed_source() {
    assert_eq!(
        empty_state_text(None),
        t("empty.no_services_reported"),
        "a healthy never-reported inventory says so plainly"
    );
    assert_eq!(
        empty_state_text(Some(&[])),
        t("empty.no_services_reported"),
        "healthy sources with no rows is still the plain empty copy"
    );
    let failed = empty_state_text(Some(&failed_source()));
    assert!(
        failed.contains(t("source.unavailable_title")),
        "a typed failure names itself: {failed}"
    );
    assert!(
        failed.contains(t("feedback.timed_out")),
        "the failure reason travels with the title: {failed}"
    );
}

#[test]
fn status_line_and_header_spell_the_active_sort() {
    let mut shell = shelved_shell(&[
        service_item("svc-a", "alpha", ServiceStatus::Active),
        service_item("svc-b", "beta", ServiceStatus::Failed),
    ]);
    assert_eq!(
        status_line_text(&shell, 2),
        format!("2 {} · provider order", t("svc.noun"))
    );
    shell.set_info_sort(InfoTable::Services, InfoSortCol::Name);
    assert_eq!(
        status_line_text(&shell, 2),
        format!(
            "2 {} · {} {}",
            t("svc.noun"),
            t("common.name"),
            SortDir::Asc.label()
        )
    );
    let header = header_label(
        &super::Column {
            sort: Some(InfoSortCol::Name),
            label: t("common.service").to_owned(),
            width_px: 260.0,
        },
        shell.services_sort,
    );
    assert_eq!(header, format!("{} ▲", t("common.service")));
    // A descending sort flips the arrow; other columns stay bare.
    shell.set_info_sort(InfoTable::Services, InfoSortCol::Name);
    let flipped = header_label(
        &super::Column {
            sort: Some(InfoSortCol::Name),
            label: t("common.service").to_owned(),
            width_px: 260.0,
        },
        shell.services_sort,
    );
    assert_eq!(flipped, format!("{} ▼", t("common.service")));
}

// ---- wired: fold → rows, sort click, row/keyboard selection, idle ----

#[test]
fn folded_rows_render_then_refresh_and_idle_frames_redraw_nothing() {
    let (mut app, events) = headless_services_app();
    route_to_services(&mut app);
    push_services(
        &events,
        vec![
            service_item("svc-b", "beta", ServiceStatus::Active),
            service_item("svc-a", "alpha", ServiceStatus::Failed),
        ],
    );
    // Frame 1 folds the batch in PreUpdate and mounts the page in Update;
    // the assertions after two frames see already-folded, already-painted rows.
    app.update();
    app.update();

    let rows = row_targets(&mut app);
    assert_eq!(
        rows,
        [(0, "svc-b".to_owned()), (1, "svc-a".to_owned())],
        "rows render in provider order until a sort is picked"
    );
    assert_eq!(
        status_line(&mut app),
        format!("2 {} · provider order", t("svc.noun"))
    );

    // Idle frames: a quiet port must not rebuild the body (entity identity is
    // the observable — a repaint despawns and respawns rows).
    let before = row_entities(&mut app);
    app.update();
    app.update();
    let idle = row_entities(&mut app);
    assert_eq!(before, idle, "no fold, no repaint");

    // A services fold refreshes the row set through the observer.
    push_services(
        &events,
        vec![
            service_item("svc-z", "zulu", ServiceStatus::Active),
            service_item("svc-a", "alpha", ServiceStatus::Active),
            service_item("svc-m", "mike", ServiceStatus::Inactive),
        ],
    );
    app.update();
    app.update();
    let refreshed = row_targets(&mut app);
    assert_eq!(
        refreshed,
        [
            (0, "svc-z".to_owned()),
            (1, "svc-a".to_owned()),
            (2, "svc-m".to_owned()),
        ],
        "the fold observer repainted the body from the new projection"
    );
    assert_eq!(
        status_line(&mut app),
        format!("3 {} · provider order", t("svc.noun"))
    );
}

#[test]
fn sort_click_routes_through_the_shell_and_keeps_selection_on_target() {
    let (mut app, events) = headless_services_app();
    route_to_services(&mut app);
    push_services(
        &events,
        vec![
            service_item("svc-b", "beta", ServiceStatus::Active),
            service_item("svc-a", "alpha", ServiceStatus::Active),
            service_item("svc-c", "gamma", ServiceStatus::Active),
        ],
    );
    app.update();
    app.update();

    // Select the middle provider-order row ("svc-a") through the row-click
    // tail; the visual row resolved ONLY through sorted_service_at.
    app.world_mut().trigger(ServiceRowClicked(1));
    app.update();
    assert_eq!(
        selected_row_target(&mut app).as_deref(),
        Some("svc-a"),
        "the clicked visual row maps to its own target"
    );

    // Header sort: the shell slot flips, the rows reorder, and the highlight
    // stays on the SAME target at its NEW row (no selection drift).
    app.world_mut()
        .trigger(ServiceSortClicked(InfoSortCol::Name));
    app.update();
    app.update();
    assert_eq!(
        shell_services_sort(&mut app),
        Some((InfoSortCol::Name, SortDir::Asc)),
        "the observer routed the click through the shell's sort entry"
    );
    assert_eq!(
        row_targets(&mut app),
        [
            (0, "svc-a".to_owned()),
            (1, "svc-b".to_owned()),
            (2, "svc-c".to_owned()),
        ],
        "rows re-projected through the shared sort"
    );
    assert_eq!(
        selected_row_target(&mut app).as_deref(),
        Some("svc-a"),
        "the selected target id survived the reorder"
    );
}

#[test]
fn keyboard_moves_clamp_and_click_targets_translate_via_sorted_service_at() {
    let (mut app, events) = headless_services_app();
    route_to_services(&mut app);
    push_services(
        &events,
        vec![
            service_item("svc-b", "beta", ServiceStatus::Active),
            service_item("svc-a", "alpha", ServiceStatus::Active),
        ],
    );
    app.update();
    app.update();

    // Sorted by name: alpha(svc-a) row 0, beta(svc-b) row 1.
    app.world_mut()
        .trigger(ServiceSortClicked(InfoSortCol::Name));
    app.update();
    // No selection yet: a move enters at the first row.
    app.world_mut().trigger(ServiceSelectionMoved(1));
    app.update();
    assert_eq!(selected_row_target(&mut app).as_deref(), Some("svc-a"));
    // Down moves to the last row; further down saturates there.
    app.world_mut().trigger(ServiceSelectionMoved(1));
    app.update();
    assert_eq!(selected_row_target(&mut app).as_deref(), Some("svc-b"));
    app.world_mut().trigger(ServiceSelectionMoved(5));
    app.update();
    assert_eq!(
        selected_row_target(&mut app).as_deref(),
        Some("svc-b"),
        "the cursor saturates at the last row"
    );
    app.world_mut().trigger(ServiceSelectionMoved(-9));
    app.update();
    assert_eq!(
        selected_row_target(&mut app).as_deref(),
        Some("svc-a"),
        "the cursor saturates at the first row"
    );
    // A click on a visual row resolves through the shell accessor under the
    // ACTIVE sort (row 1 under Name-ascending is svc-b).
    app.world_mut().trigger(ServiceRowClicked(1));
    app.update();
    assert_eq!(selected_row_target(&mut app).as_deref(), Some("svc-b"));
    // A click past the end is rejected, never wrapped to another row.
    app.world_mut().trigger(ServiceRowClicked(9));
    app.update();
    assert_eq!(
        selected_row_target(&mut app).as_deref(),
        Some("svc-b"),
        "an out-of-range click leaves the selection untouched"
    );
}

#[test]
fn selection_clears_honestly_when_the_target_leaves_the_inventory() {
    let (mut app, events) = headless_services_app();
    route_to_services(&mut app);
    push_services(
        &events,
        vec![
            service_item("svc-a", "alpha", ServiceStatus::Active),
            service_item("svc-b", "beta", ServiceStatus::Active),
        ],
    );
    app.update();
    app.update();
    app.world_mut().trigger(ServiceRowClicked(1));
    app.update();
    assert_eq!(selected_row_target(&mut app).as_deref(), Some("svc-b"));

    // The next fold drops svc-b entirely.
    push_services(
        &events,
        vec![service_item("svc-a", "alpha", ServiceStatus::Active)],
    );
    app.update();
    app.update();
    assert_eq!(
        selected_row_target(&mut app),
        None,
        "a vanished target deselects instead of jumping to a neighbor"
    );
    assert_eq!(
        app.world().resource::<ServiceSelection>().target,
        None,
        "the selection resource stays id-keyed and honest"
    );
}
