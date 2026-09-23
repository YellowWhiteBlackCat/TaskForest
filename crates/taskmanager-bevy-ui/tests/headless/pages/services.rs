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
use bevy::ui_widgets::Activate;
use taskmanager_application::i18n::t;
use taskmanager_application::{
    CorrelatedServiceEvent, HostTelemetryRequest, PlatformClient, PlatformEvent, PlatformFacets,
    PlatformHandle, ServiceEvent, SystemFacets,
};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::ProviderId;
use taskmanager_core::core::services::{ServiceAction, ServiceItem, ServiceStatus};
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_core::core::target::ServiceId;
use taskmanager_platform_contract::{
    CapabilityCatalog, CapabilityDescriptor, CapabilityId, CapabilitySnapshot, CapabilityStatus,
    EventEnvelope, EventPort, EventPortError, EventSequence, PartialSourceSnapshot,
    RequestEnvelope, RequestId, RequestPort, SubmissionError,
};

use taskmanager_shell::{InfoSortCol, InfoTable, ShellApp, SortDir};
use taskmanager_theme::Theme;

use super::{
    ServiceRestartButton, ServiceRowClicked, ServiceSelection, ServiceSelectionMoved,
    ServiceSortClicked, ServiceStartButton, ServiceStopButton, ServicesRowMarker,
    ServicesSortHeader, ServicesStatusLine, StatusChip, chip_fill, empty_state_text, header_label,
    moved_row, selected_row, service_chip, service_rows, sorted_direction, status_line_text,
};
use crate::app::{FrontendTrack, Page, Route, RouteChanged};
use crate::palette::ui_palette;
use crate::runtime::{RuntimeCache, SharedRuntime};
use crate::window::FrontendWindowPlugin;
use crate::window::tests::HeadlessFrontendPlugins;
use taskmanager_application::PlatformEventBatch;

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
    shell.apply_platform_batch(PlatformEventBatch {
        service_events: vec![correlated_service_snapshot(items.to_vec())],
        ..PlatformEventBatch::default()
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
pub(crate) struct ScriptedEvents(Mutex<VecDeque<EventEnvelope<PlatformEvent>>>);

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

pub(crate) fn headless_services_app() -> (App, Arc<ScriptedEvents>) {
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

pub(crate) fn push_services(events: &ScriptedEvents, items: Vec<ServiceItem>) {
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
pub(crate) fn route_to_services(app: &mut App) {
    app.world_mut().resource_mut::<Route>().page = Page::Services;
    app.world_mut().commands().trigger(RouteChanged);
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
    // The header label stays the pure column word; the sort rides a typed
    // direction (the render layer turns it into the semantic arrow plate).
    let column = super::Column {
        sort: Some(InfoSortCol::Name),
        label: t("common.service").to_owned(),
        width_px: 260.0,
    };
    assert_eq!(header_label(&column), t("common.service"));
    assert_eq!(sorted_direction(&column, shell.services_sort), Some(false));
    // A descending sort flips the direction; an unsorted column stays bare.
    // Clicking the active column again toggles its direction (shell law).
    shell.set_info_sort(InfoTable::Services, InfoSortCol::Name);
    assert_eq!(sorted_direction(&column, shell.services_sort), Some(true));
    let other = super::Column {
        sort: Some(InfoSortCol::Status),
        label: t("common.service").to_owned(),
        width_px: 260.0,
    };
    assert_eq!(sorted_direction(&other, shell.services_sort), None);
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
    let status = status_line(&mut app);
    assert!(
        status == "2 services · provider order"
            || status == "2 服务 · provider order"
            || status == format!("2 {} · provider order", t("svc.noun")),
        "status line matches: {status}"
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

#[test]
fn services_toolbar_mounts_lifecycle_control_buttons() {
    let (mut app, events) = headless_services_app();
    route_to_services(&mut app);
    push_services(
        &events,
        vec![service_item("svc-a", "alpha", ServiceStatus::Active)],
    );
    app.update();
    app.update();

    let start_buttons: Vec<_> = app
        .world_mut()
        .query_filtered::<Entity, With<ServiceStartButton>>()
        .iter(app.world())
        .collect();
    let stop_buttons: Vec<_> = app
        .world_mut()
        .query_filtered::<Entity, With<ServiceStopButton>>()
        .iter(app.world())
        .collect();
    let restart_buttons: Vec<_> = app
        .world_mut()
        .query_filtered::<Entity, With<ServiceRestartButton>>()
        .iter(app.world())
        .collect();

    assert_eq!(
        start_buttons.len(),
        1,
        "services toolbar mounts exactly one Start button"
    );
    assert_eq!(
        stop_buttons.len(),
        1,
        "services toolbar mounts exactly one Stop button"
    );
    assert_eq!(
        restart_buttons.len(),
        1,
        "services toolbar mounts exactly one Restart button"
    );
}

#[test]
fn service_row_pointer_click_selects_row() {
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

    let entities = row_entities(&mut app);
    assert_eq!(entities.len(), 2);
    // Click on row 1 ("svc-b") via pointer activation
    let row_target = entities[1].0;
    app.world_mut()
        .commands()
        .trigger(Activate { entity: row_target });
    app.update();
    app.update();

    assert_eq!(
        selected_row_target(&mut app).as_deref(),
        Some("svc-b"),
        "pointer click on service row selects it and applies highlight"
    );
}

#[test]
fn sort_header_pointer_click_sorts_column() {
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

    // Find the Name sort header entity
    let header_entity = app
        .world_mut()
        .query_filtered::<(Entity, &ServicesSortHeader), ()>()
        .iter(app.world())
        .find(|(_, h)| h.0 == Some(InfoSortCol::Name))
        .map(|(e, _)| e)
        .expect("name sort header exists");

    app.world_mut().commands().trigger(Activate {
        entity: header_entity,
    });
    app.update();
    app.update();

    assert_eq!(
        shell_services_sort(&mut app),
        Some((InfoSortCol::Name, SortDir::Asc)),
        "pointer click on sort header sets info sort"
    );
    assert_eq!(
        row_targets(&mut app),
        [(0, "svc-a".to_owned()), (1, "svc-b".to_owned()),],
        "rows re-project through name sort"
    );
}

#[test]
fn service_control_buttons_arm_service_actions() {
    let (mut app, events) = headless_services_app();
    route_to_services(&mut app);
    push_services(
        &events,
        vec![service_item("svc-a", "alpha", ServiceStatus::Active)],
    );
    app.update();
    app.update();

    // Select row 0 ("svc-a")
    app.world_mut().trigger(ServiceRowClicked(0));
    app.update();

    // Start button
    let start_btn = app
        .world_mut()
        .query_filtered::<Entity, With<ServiceStartButton>>()
        .iter(app.world())
        .next()
        .expect("start button exists");
    app.world_mut()
        .commands()
        .trigger(Activate { entity: start_btn });
    app.update();

    assert_eq!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .pending_service_control()
            .map(|s| (s.service_id.as_str(), s.action)),
        Some(("svc-a", ServiceAction::Start))
    );

    // Stop button
    let stop_btn = app
        .world_mut()
        .query_filtered::<Entity, With<ServiceStopButton>>()
        .iter(app.world())
        .next()
        .expect("stop button exists");
    app.world_mut()
        .commands()
        .trigger(Activate { entity: stop_btn });
    app.update();

    assert_eq!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .pending_service_control()
            .map(|s| (s.service_id.as_str(), s.action)),
        Some(("svc-a", ServiceAction::Stop))
    );

    // Restart button
    let restart_btn = app
        .world_mut()
        .query_filtered::<Entity, With<ServiceRestartButton>>()
        .iter(app.world())
        .next()
        .expect("restart button exists");
    app.world_mut().commands().trigger(Activate {
        entity: restart_btn,
    });
    app.update();

    assert_eq!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .pending_service_control()
            .map(|s| (s.service_id.as_str(), s.action)),
        Some(("svc-a", ServiceAction::Restart))
    );

    // The definition also names enable/disable. Those two verbs live in the
    // production action menu (the toolbar carries start/stop/restart); each
    // committed pick must arm the same shared gate against the frozen unit.
    use super::menu::{MENU_ACTIONS, ServiceMenuCtx};
    use crate::menu_modal::ActionMenuContext;
    let menu_ctx = ServiceMenuCtx(service_item("svc-a", "alpha", ServiceStatus::Active));
    for action in [ServiceAction::Enable, ServiceAction::Disable] {
        let pick = MENU_ACTIONS
            .iter()
            .position(|candidate| *candidate == action)
            .expect("the shared services menu lists every lifecycle verb");
        let mut track = app.world_mut().non_send_mut::<FrontendTrack>();
        let effects = menu_ctx.commit(pick, &mut track.shell);
        assert!(
            effects.is_empty(),
            "an inventory menu verb only arms the shared gate"
        );
        assert_eq!(
            track
                .shell
                .pending_service_control()
                .map(|s| (s.service_id.as_str(), s.action)),
            Some(("svc-a", action)),
            "the {action:?} menu pick must target the frozen unit"
        );
    }
}

/// The dependency-DAG definition's ordering-cycle clause: a typed `Before`
/// cycle between inventory units paints the warning plate on exactly those
/// rows through the shared cycle fold, while an acyclic unit stays unmarked.
#[test]
fn ordering_cycle_members_paint_the_warning_plate() {
    use bevy::ecs::hierarchy::ChildOf;
    use taskmanager_core::core::services::{
        ServiceRelationEdge, ServiceRelationGraph, ServiceRelationKind,
    };
    use taskmanager_ui_contract::IconId;

    let cyclic = |id: &str, name: &str, other: &str| {
        service_item(id, name, ServiceStatus::Active).with_relations(
            ServiceRelationGraph::from_edges([ServiceRelationEdge::new(
                ServiceRelationKind::Before,
                other,
            )]),
        )
    };
    let (mut app, events) = headless_services_app();
    route_to_services(&mut app);
    push_services(
        &events,
        vec![
            cyclic("cycle-a.service", "cycle-a", "cycle-b.service"),
            cyclic("cycle-b.service", "cycle-b", "cycle-a.service"),
            service_item("plain.service", "plain", ServiceStatus::Active),
        ],
    );
    app.update();
    app.update();

    let mut plate_query = app
        .world_mut()
        .query::<(Entity, &crate::icons::IconPlate)>();
    let alert_entities: Vec<Entity> = plate_query
        .iter(app.world())
        .filter(|(_, plate)| plate.0 == IconId::Alert)
        .map(|(entity, _)| entity)
        .collect();
    assert!(
        !alert_entities.is_empty(),
        "a typed ordering cycle must paint its warning plate"
    );
    let mut marked: Vec<String> = Vec::new();
    for entity in alert_entities {
        let mut current = entity;
        loop {
            if let Some(marker) = app.world().get::<ServicesRowMarker>(current) {
                marked.push(marker.1.as_str().to_owned());
                break;
            }
            match app.world().get::<ChildOf>(current) {
                Some(parent) => current = parent.0,
                None => break,
            }
        }
    }
    marked.sort_unstable();
    assert_eq!(
        marked,
        vec!["cycle-a.service".to_owned(), "cycle-b.service".to_owned()],
        "exactly the ordering-cycle members must carry the warning plate"
    );
}
