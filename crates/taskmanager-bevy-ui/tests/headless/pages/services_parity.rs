//! test-intent: behavior
//!
//! Services-page parity behavior for the Bevy frontend: the typed status dot
//! with its breathing pulse, the unit-type filter bar that slides instead of
//! wrapping, the icon-bearing action capsules, the expandable dependency tree,
//! and the service-log surfaces (RFC3339 stamps, level-keyword highlighting
//! and the follow lock with its "return to latest" capsule).
//!
//! Every assertion here is a behavior or a typed state — never a source-text
//! check, never a sleep, never a host-specific value.
//!
//! Mounted from `pages/services.rs` (the Services page surface).

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::color::Color;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::query::With;
use bevy::scene::{ScenePlugin, WorldSceneExt};
use bevy::text::Font;
use bevy::ui::prelude::{FlexWrap, Node, OverflowAxis, Val};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, ScrollArea};
use taskmanager_application::i18n::t;
use taskmanager_core::core::services::{
    ServiceDeps, ServiceItem, ServiceLogEntry, ServiceLogLevel, ServiceLogQuery,
    ServiceLogStreamSnapshot, ServiceLogStreamState, ServiceRelationKind, ServiceStatus,
};
use taskmanager_core::core::target::ServiceId;
use taskmanager_platform_contract::RequestId;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::UiSize;

use super::action_glyph::{ServiceActionGlyph, ServiceActionGlyphMark};
use super::deps_tree::{ServiceDepsExpansion, ServiceDepsToggle, dependency_tree};
use super::log_follow::{ServiceLogReturnLatestButton, ServiceLogScrolled};
use super::log_panel::LogPanelRepaintRequired;
use super::log_text::{
    LogInk, LogSegmentKind, ServiceLogInk, log_segments, log_stamp_text, rfc3339_utc,
};
use super::status_dot::{
    ServiceStatusDot, pulse_opacity, pulse_phase, status_dot_alpha, status_dot_color,
};
use super::tests::{headless_services_app, push_services, route_to_services};
use super::unit_filter::{
    ServiceUnitFilter, ServiceUnitFilterChoice, ServiceUnitFilterSlot, ServiceUnitType,
    UnitFilterChoice, filter_unit_rows, unit_type_of,
};
use crate::app::FrontendTrack;
use crate::palette::ui_palette;
use taskmanager_application::CorrelatedServiceEvent;
use taskmanager_application::PlatformEventBatch;
use taskmanager_application::ServiceEvent;
use taskmanager_core::core::services::ServiceLogLevelFilter;
use taskmanager_core::core::services::ServiceLogTimeFilter;
use taskmanager_core::core::services::ServiceRelationEdge;
use taskmanager_core::core::services::ServiceRelationGraph;
use taskmanager_platform_contract::CapabilityId;
use taskmanager_platform_contract::EventSequence;
use taskmanager_platform_contract::PartialSourceSnapshot;
use taskmanager_shell::ShellApp;

// ---- fixtures ------------------------------------------------------------

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

fn palette() -> crate::palette::UiPalette {
    ui_palette(&Theme::dark(), UiSize::Standard)
}

fn rem_of(value: Val) -> f32 {
    match value {
        Val::Rem(rem) => rem,
        _ => f32::NAN,
    }
}

/// The ink channels of one color, without the alpha.
fn channels(color: Color) -> (f32, f32, f32) {
    let srgba = color.to_srgba();
    (srgba.red, srgba.green, srgba.blue)
}

/// The alpha channel of one color.
fn alpha(color: Color) -> f32 {
    color.to_srgba().alpha
}

/// The painted status dots of the mounted Services page.
fn painted_dots(app: &mut App) -> Vec<(super::StatusChip, Color, f32)> {
    app.world_mut()
        .query::<(&ServiceStatusDot, &bevy::ui::BackgroundColor, &Node)>()
        .iter(app.world())
        .map(|(dot, fill, node)| (dot.0, fill.0, rem_of(node.width)))
        .collect()
}

// ---- BEVY-138: the typed status dot ---------------------------------------

#[test]
fn the_status_dot_paints_each_service_state_in_its_own_ink_and_extent() {
    let (mut app, events) = headless_services_app();
    route_to_services(&mut app);
    push_services(
        &events,
        vec![
            service_item("running.service", "running", ServiceStatus::Active),
            service_item("stopped.service", "stopped", ServiceStatus::Inactive),
            service_item("failed.service", "failed", ServiceStatus::Failed),
        ],
    );
    app.update();
    app.update();

    let palette = palette();
    let dots = painted_dots(&mut app);
    assert_eq!(
        dots.len(),
        3,
        "every service row carries one status dot: {dots:?}"
    );

    let running = dots
        .iter()
        .find(|(chip, _, _)| *chip == super::StatusChip::Positive)
        .expect("the running row paints its dot");
    let stopped = dots
        .iter()
        .find(|(chip, _, _)| *chip == super::StatusChip::Idle)
        .expect("the stopped row paints its dot");
    let failed = dots
        .iter()
        .find(|(chip, _, _)| *chip == super::StatusChip::Negative)
        .expect("the failed row paints its dot");

    // The failed unit wears the destructive red and never breathes.
    assert_eq!(
        channels(failed.1),
        channels(status_dot_color(super::StatusChip::Negative, &palette)),
        "a failed unit wears the destructive ink"
    );
    assert_eq!(
        alpha(failed.1),
        status_dot_alpha(super::StatusChip::Negative),
        "a failed unit is never quietened"
    );

    // The running unit wears the accent and breathes: its alpha follows the
    // typed pulse curve, never a wall-clock guess.
    assert_eq!(
        channels(running.1),
        channels(status_dot_color(super::StatusChip::Positive, &palette)),
        "a running unit wears the accent ink"
    );
    let phase = app
        .world()
        .resource::<super::status_dot::ServicePulse>()
        .elapsed;
    let expected =
        status_dot_alpha(super::StatusChip::Positive) * pulse_opacity(pulse_phase(phase));
    assert!(
        (alpha(running.1) - expected).abs() < 0.0001,
        "the running dot's alpha follows the typed breathing curve: {} vs {expected}",
        alpha(running.1)
    );

    // The hierarchy: a stopped unit reads smaller than a running one.
    assert!(
        stopped.2 < running.2,
        "a stopped unit reads smaller than a running one: {} vs {}",
        stopped.2,
        running.2
    );
}

#[test]
fn the_status_dot_breathes_through_its_typed_phase() {
    let phase = pulse_phase(0.0);
    let far = pulse_phase(super::status_dot::PULSE_PERIOD_SECS * 0.5);
    assert!(
        (phase - far).abs() > 0.5,
        "the breathing curve travels: {phase} vs {far}"
    );
    assert!(
        pulse_opacity(0.0) < pulse_opacity(1.0),
        "the breathing curve dims and brightens"
    );
    assert!(
        (0.55..=1.0).contains(&pulse_opacity(phase)),
        "the breathing opacity stays inside its band"
    );
}

// ---- BEVY-141: the icon-bearing action capsules ---------------------------

/// The ink one lifecycle mark paints: the filled marks carry their fill,
/// the ring carries its border.
fn mark_ink(app: &mut App, glyph: ServiceActionGlyph) -> Option<(f32, f32, f32)> {
    app.world_mut()
        .query::<(
            &ServiceActionGlyphMark,
            &bevy::ui::BackgroundColor,
            Option<&bevy::ui::BorderColor>,
        )>()
        .iter(app.world())
        .find(|(mark, _, _)| mark.0 == glyph)
        .map(|(_, fill, border)| match border {
            Some(border) if border.top != bevy::color::Color::NONE => channels(border.top),
            _ => channels(fill.0),
        })
}

#[test]
fn the_lifecycle_capsules_carry_a_semantic_mark() {
    let (mut app, events) = headless_services_app();
    route_to_services(&mut app);
    push_services(
        &events,
        vec![service_item(
            "alpha.service",
            "alpha",
            ServiceStatus::Active,
        )],
    );
    app.update();
    app.update();

    let palette = palette();
    let marks = app
        .world_mut()
        .query::<&ServiceActionGlyphMark>()
        .iter(app.world())
        .count();
    assert_eq!(
        marks, 3,
        "the start, stop and restart capsules each carry a semantic mark"
    );

    // Without a selected unit the group is inert: the marks dim, so an
    // unarmed action never looks armed.
    let dim = channels(palette.dim_color);
    assert_eq!(
        mark_ink(&mut app, ServiceActionGlyph::Start),
        Some(dim),
        "an inert action group dims its marks"
    );
    assert_eq!(
        mark_ink(&mut app, ServiceActionGlyph::Stop),
        Some(dim),
        "an inert action group dims its marks"
    );
    assert_eq!(
        mark_ink(&mut app, ServiceActionGlyph::Restart),
        Some(dim),
        "an inert action group dims its marks"
    );
}

#[test]
fn selecting_a_service_arms_the_lifecycle_marks() {
    let (mut app, events) = headless_services_app();
    route_to_services(&mut app);
    push_services(
        &events,
        vec![service_item(
            "alpha.service",
            "alpha",
            ServiceStatus::Active,
        )],
    );
    app.update();
    app.update();

    app.world_mut().trigger(super::ServiceRowClicked(0));
    app.update();
    app.update();

    let palette = palette();
    assert_eq!(
        mark_ink(&mut app, ServiceActionGlyph::Start),
        Some(channels(palette.accent)),
        "a selected unit arms the start mark with the accent ink"
    );
    assert_eq!(
        mark_ink(&mut app, ServiceActionGlyph::Stop),
        Some(channels(palette.danger_color)),
        "a selected unit arms the stop mark with the destructive ink"
    );
    assert_eq!(
        mark_ink(&mut app, ServiceActionGlyph::Restart),
        Some(channels(palette.body_color)),
        "a selected unit arms the restart mark with the body ink"
    );
}

// ---- BEVY-143: the unit-type filter bar -----------------------------------

#[test]
fn the_unit_type_is_derived_from_the_unit_name_and_filters_the_rows() {
    assert_eq!(unit_type_of("sshd.service"), ServiceUnitType::Service);
    assert_eq!(
        unit_type_of("systemd-journald.socket"),
        ServiceUnitType::Socket
    );
    assert_eq!(unit_type_of("apt-daily.timer"), ServiceUnitType::Timer);
    assert_eq!(unit_type_of("multi-user.target"), ServiceUnitType::Target);
    assert_eq!(
        unit_type_of("plain-unit-without-a-suffix"),
        ServiceUnitType::Other,
        "a unit without a known suffix is never guessed"
    );

    let rows = vec![
        super::ServiceRowModel {
            target: ServiceId::new("a.service"),
            name: "a".to_owned(),
            status: ServiceStatus::Active,
            description: String::new(),
            cycle: false,
        },
        super::ServiceRowModel {
            target: ServiceId::new("b.socket"),
            name: "b".to_owned(),
            status: ServiceStatus::Active,
            description: String::new(),
            cycle: false,
        },
    ];
    let filter = ServiceUnitFilter {
        choice: UnitFilterChoice::Unit(ServiceUnitType::Socket),
    };
    let kept = filter_unit_rows(rows, filter);
    assert_eq!(kept.len(), 1, "the filter keeps only the socket unit");
    assert_eq!(kept[0].target.as_str(), "b.socket");
}

#[test]
fn the_filter_bar_slides_sideways_and_never_wraps() {
    let (mut app, events) = headless_services_app();
    route_to_services(&mut app);
    push_services(
        &events,
        vec![service_item(
            "alpha.service",
            "alpha",
            ServiceStatus::Active,
        )],
    );
    app.update();
    app.update();

    let slot = app
        .world_mut()
        .query_filtered::<Entity, With<ServiceUnitFilterSlot>>()
        .iter(app.world())
        .next()
        .expect("the services page mounts the unit filter bar");
    assert!(
        app.world().get::<Node>(slot).is_some(),
        "the filter slot is a layout node"
    );

    // The painted bar is the scroll viewport INSIDE the filter slot: it may
    // shrink below its content and slides sideways instead of folding into a
    // second line, so the toolbar above keeps its single row in a narrow
    // window. The bar is resolved through the slot's own child — the
    // navigation strip also owns a horizontal scroll viewport, and "the first
    // horizontal ScrollArea" would depend on archetype iteration order.
    let bar = {
        let world = app.world_mut();
        // The bar's scroll area lives inside the slot's viewport wrapper (the
        // rail is the wrapper's other child), so the lookup walks the slot's
        // DESCENDANTS — a probe over direct children would stop at the
        // wrapper and never reach the scroll area.
        let mut pending: Vec<bevy::ecs::entity::Entity> = world
            .get::<Children>(slot)
            .expect("the filter slot owns the bar")
            .to_vec();
        let mut found = None;
        while let Some(entity) = pending.pop() {
            let node = world.get::<Node>(entity);
            let horizontal = node.is_some_and(|node| node.overflow.x == OverflowAxis::Scroll);
            if horizontal && world.get::<ScrollArea>(entity).is_some() {
                found = node.cloned();
                break;
            }
            if let Some(children) = world.get::<Children>(entity) {
                pending.extend(children.iter().copied());
            }
        }
        found.expect("the filter bar owns a horizontal scroll viewport")
    };
    assert_eq!(
        bar.flex_wrap,
        FlexWrap::NoWrap,
        "the capsule bar never folds into a second line"
    );
    assert_eq!(
        bar.min_width,
        Val::Px(0.0),
        "the capsule bar may shrink below its content so it can slide"
    );
    assert_eq!(
        bar.overflow.x,
        OverflowAxis::Scroll,
        "the capsule bar scrolls sideways"
    );
}

#[test]
fn choosing_a_unit_type_filters_the_service_rows() {
    let (mut app, events) = headless_services_app();
    route_to_services(&mut app);
    push_services(
        &events,
        vec![
            service_item("alpha.service", "alpha", ServiceStatus::Active),
            service_item("beta.socket", "beta", ServiceStatus::Active),
        ],
    );
    app.update();
    app.update();

    let socket_capsule = app
        .world_mut()
        .query_filtered::<(Entity, &ServiceUnitFilterChoice), ()>()
        .iter(app.world())
        .find(|(_, choice)| choice.0 == UnitFilterChoice::Unit(ServiceUnitType::Socket))
        .map(|(entity, _)| entity)
        .expect("the filter bar offers the socket unit type");
    app.world_mut().commands().trigger(Activate {
        entity: socket_capsule,
    });
    app.update();
    app.update();

    assert_eq!(
        app.world().resource::<ServiceUnitFilter>().choice,
        UnitFilterChoice::Unit(ServiceUnitType::Socket),
        "the chosen capsule owns the filter"
    );
    let targets: Vec<String> = app
        .world_mut()
        .query_filtered::<&super::ServicesRowMarker, ()>()
        .iter(app.world())
        .map(|marker| marker.1.as_str().to_owned())
        .collect();
    assert_eq!(
        targets,
        vec!["beta.socket".to_owned()],
        "the table shows only the chosen unit type"
    );
}

// ---- BEVY-142: the dependency tree ----------------------------------------

fn relation_deps() -> ServiceDeps {
    let mut deps = ServiceDeps::default();
    deps.replace_relation_targets(
        ServiceRelationKind::Requires,
        [ServiceId::new("dep.service")],
    );
    deps
}

#[test]
fn the_dependency_tree_nests_the_relations_the_inventory_knows() {
    let mut shell = ShellApp::new();
    shell.apply_platform_batch(PlatformEventBatch {
        service_events: vec![CorrelatedServiceEvent {
            request_id: RequestId::MIN,
            capability: CapabilityId::SERVICES,
            provider: None,
            sequence: EventSequence::new(1),
            observed_at_ms: 1,
            event: ServiceEvent::Snapshot(PartialSourceSnapshot {
                items: vec![
                    service_item("root.service", "root", ServiceStatus::Active),
                    service_item("dep.service", "dep", ServiceStatus::Active).with_relations(
                        ServiceRelationGraph::from_edges([ServiceRelationEdge::new(
                            ServiceRelationKind::Requires,
                            "leaf.service",
                        )]),
                    ),
                ],
                sources: Vec::new(),
            }),
        }],
        ..PlatformEventBatch::default()
    });

    let root = ServiceId::new("root.service");
    let tree = dependency_tree(&shell, &root, &relation_deps());
    let requires = tree
        .children
        .iter()
        .find(|node| node.relation == Some(ServiceRelationKind::Requires))
        .expect("the tree carries the requires section");
    assert_eq!(requires.count, 1, "the section counts its related units");
    let dep = &requires.children[0];
    assert_eq!(
        dep.label, "dep",
        "a related unit is named from the inventory"
    );
    assert!(
        !dep.children.is_empty(),
        "a related unit the inventory knows carries its own relations"
    );
    let nested = &dep.children[0];
    assert_eq!(nested.relation, Some(ServiceRelationKind::Requires));
    assert_eq!(
        nested.children[0].label, "leaf.service",
        "the deepest branch names the unit it points at"
    );
}

#[test]
fn the_dependency_panel_opens_a_section_and_closes_it_again() {
    let (mut app, events) = headless_services_app();
    route_to_services(&mut app);
    push_services(
        &events,
        vec![service_item(
            "alpha.service",
            "alpha",
            ServiceStatus::Active,
        )],
    );
    app.update();
    app.update();

    {
        let mut track = app.world_mut().non_send_mut::<FrontendTrack>();
        let target = ServiceId::new("alpha.service");
        let request = RequestId::new(1).expect("valid request id");
        track
            .shell
            .service_dependencies
            .begin(request, target.clone());
        let _ = track
            .shell
            .service_dependencies
            .resolve(request, target, relation_deps());
    }
    app.world_mut()
        .commands()
        .trigger(super::dependencies_panel::DependenciesPanelRepaintRequired);
    app.update();
    app.update();

    let painted: Vec<String> = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .collect();
    assert!(
        painted.iter().any(|text| text == "dep.service"),
        "the opened section paints its related unit: {painted:?}"
    );

    // The section's disclosure closes it again.
    let toggle = app
        .world_mut()
        .query_filtered::<(Entity, &ServiceDepsToggle), ()>()
        .iter(app.world())
        .find(|(_, toggle)| toggle.0.ends_with("/requires"))
        .map(|(entity, _)| entity)
        .expect("the section paints a disclosure");
    app.world_mut()
        .commands()
        .trigger(Activate { entity: toggle });
    app.update();
    app.update();

    let expansion = app.world().resource::<ServiceDepsExpansion>();
    assert!(
        !expansion.is_expanded("alpha.service/requires", true),
        "the disclosure closes the section"
    );
    let painted: Vec<String> = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .collect();
    assert!(
        !painted.iter().any(|text| text == "dep.service"),
        "a closed section hides its related unit: {painted:?}"
    );
}

// ---- BEVY-144 / BEVY-140: the log text ------------------------------------

#[test]
fn the_realtime_stamp_is_rendered_as_rfc3339_utc() {
    assert_eq!(
        rfc3339_utc(1_700_000_000_000_000),
        "2023-11-14T22:13:20.000000Z",
        "the provider stamp renders as RFC3339 UTC"
    );
    assert_eq!(
        log_stamp_text(None),
        "--:--:--",
        "an entry without a realtime stamp never fabricates one"
    );
    assert_eq!(
        log_stamp_text(Some(1_700_000_000_123_456)),
        "2023-11-14T22:13:20.123456Z",
        "the sub-second part survives"
    );
}

#[test]
fn the_log_message_splits_the_level_keywords_out_of_the_body() {
    let segments = log_segments("ERROR: unit refused to start");
    assert_eq!(segments.len(), 2, "one keyword and one body run");
    assert_eq!(segments[0].text, "ERROR");
    assert_eq!(segments[0].kind, LogSegmentKind::Error);
    assert_eq!(segments[1].text, ": unit refused to start");
    assert_eq!(segments[1].kind, LogSegmentKind::Plain);

    let warned = log_segments("warning: retrying");
    assert_eq!(warned[0].kind, LogSegmentKind::Warning);
    assert_eq!(warned[0].text, "warning");

    // A keyword inside a longer word is ordinary text.
    let ordinary = log_segments("erroneous input");
    assert_eq!(ordinary.len(), 1, "no keyword inside a longer word");
    assert_eq!(ordinary[0].kind, LogSegmentKind::Plain);
}

// ---- BEVY-139: the follow lock and its capsule -----------------------------

fn log_entry(index: usize, message: &str) -> ServiceLogEntry {
    ServiceLogEntry {
        cursor: format!("j:{index:04}"),
        realtime_timestamp_micros: Some(1_700_000_000_000_000),
        priority: Some(6),
        level: ServiceLogLevel::Info,
        message: message.to_owned(),
    }
}

fn open_log_stream(app: &mut App, service: &ServiceId) {
    {
        let mut track = app.world_mut().non_send_mut::<FrontendTrack>();
        let _ = track.shell.open_service_log_for(service.clone());
        let query = ServiceLogQuery {
            service_id: service.clone(),
            level: ServiceLogLevelFilter::All,
            time: ServiceLogTimeFilter::All,
            after_cursor: None,
        };
        let snapshot = ServiceLogStreamSnapshot {
            query,
            state: ServiceLogStreamState::from_query_entries(
                &ServiceLogQuery {
                    service_id: service.clone(),
                    level: ServiceLogLevelFilter::All,
                    time: ServiceLogTimeFilter::All,
                    after_cursor: None,
                },
                vec![log_entry(0, "ERROR: unit refused to start")],
            ),
        };
        if let Some(open) = track.shell.service_log.as_mut() {
            open.feed.apply_at(snapshot, 1_000);
        }
    }
    app.world_mut().commands().trigger(LogPanelRepaintRequired);
    app.update();
    app.update();
}

fn follow_flag(app: &mut App) -> bool {
    app.world()
        .non_send::<FrontendTrack>()
        .shell
        .service_log
        .as_ref()
        .is_some_and(|open| open.feed.follow)
}

fn capsule_count(app: &mut App) -> usize {
    app.world_mut()
        .query_filtered::<Entity, With<ServiceLogReturnLatestButton>>()
        .iter(app.world())
        .count()
}

#[test]
fn scrolling_away_from_the_newest_entry_releases_the_follow_lock() {
    let (mut app, events) = headless_services_app();
    route_to_services(&mut app);
    push_services(
        &events,
        vec![service_item(
            "alpha.service",
            "alpha",
            ServiceStatus::Active,
        )],
    );
    app.update();
    app.update();
    open_log_stream(&mut app, &ServiceId::new("alpha.service"));

    assert!(
        follow_flag(&mut app),
        "a fresh stream follows the newest entry"
    );
    assert_eq!(
        capsule_count(&mut app),
        0,
        "a pinned stream shows no return capsule"
    );

    app.world_mut()
        .commands()
        .trigger(ServiceLogScrolled { at_bottom: false });
    app.update();
    app.update();

    assert!(
        !follow_flag(&mut app),
        "scrolling away from the newest entry releases the lock"
    );
    assert_eq!(
        capsule_count(&mut app),
        1,
        "the released lock shows the return-to-latest capsule"
    );

    // The capsule floats INSIDE the bounded log region: it can never push
    // the last log line out of the viewport.
    let capsule = app
        .world_mut()
        .query_filtered::<Entity, With<ServiceLogReturnLatestButton>>()
        .iter(app.world())
        .next()
        .expect("the released lock shows the capsule");
    let mut current = capsule;
    let mut inside = false;
    loop {
        if app
            .world()
            .get::<super::log_panel::LogEntriesBox>(current)
            .is_some()
        {
            inside = true;
            break;
        }
        match app.world().get::<bevy::ecs::hierarchy::ChildOf>(current) {
            Some(parent) => current = parent.0,
            None => break,
        }
    }
    assert!(
        inside,
        "the return capsule stays inside the bounded log region"
    );
}

#[test]
fn the_return_capsule_pins_the_stream_again() {
    let (mut app, events) = headless_services_app();
    route_to_services(&mut app);
    push_services(
        &events,
        vec![service_item(
            "alpha.service",
            "alpha",
            ServiceStatus::Active,
        )],
    );
    app.update();
    app.update();
    open_log_stream(&mut app, &ServiceId::new("alpha.service"));

    app.world_mut()
        .commands()
        .trigger(ServiceLogScrolled { at_bottom: false });
    app.update();
    app.update();
    assert!(!follow_flag(&mut app));

    let capsule = app
        .world_mut()
        .query_filtered::<Entity, With<ServiceLogReturnLatestButton>>()
        .iter(app.world())
        .next()
        .expect("the released lock shows the capsule");
    app.world_mut()
        .commands()
        .trigger(Activate { entity: capsule });
    app.update();
    app.update();

    assert!(
        follow_flag(&mut app),
        "the capsule re-pins the stream at the newest entry"
    );
    assert_eq!(
        capsule_count(&mut app),
        0,
        "a re-pinned stream shows no return capsule"
    );
}

#[test]
fn scrolling_back_to_the_newest_entry_restores_the_lock() {
    let (mut app, events) = headless_services_app();
    route_to_services(&mut app);
    push_services(
        &events,
        vec![service_item(
            "alpha.service",
            "alpha",
            ServiceStatus::Active,
        )],
    );
    app.update();
    app.update();
    open_log_stream(&mut app, &ServiceId::new("alpha.service"));

    app.world_mut()
        .commands()
        .trigger(ServiceLogScrolled { at_bottom: false });
    app.update();
    app.update();
    assert!(!follow_flag(&mut app));

    app.world_mut()
        .commands()
        .trigger(ServiceLogScrolled { at_bottom: true });
    app.update();
    app.update();
    assert!(
        follow_flag(&mut app),
        "returning to the newest entry restores the lock"
    );
    assert_eq!(capsule_count(&mut app), 0);
}

#[test]
fn the_painted_log_line_owns_a_quiet_stamp_and_a_highlighted_keyword() {
    let (mut app, events) = headless_services_app();
    route_to_services(&mut app);
    push_services(
        &events,
        vec![service_item(
            "alpha.service",
            "alpha",
            ServiceStatus::Active,
        )],
    );
    app.update();
    app.update();
    open_log_stream(&mut app, &ServiceId::new("alpha.service"));

    let palette = palette();
    let runs: Vec<(String, LogInk, Color)> = app
        .world_mut()
        .query::<(&Text, &ServiceLogInk, &bevy::text::TextColor)>()
        .iter(app.world())
        .map(|(text, ink, color)| (text.0.clone(), ink.0, color.0))
        .collect();

    let stamp = runs
        .iter()
        .find(|(text, ink, _)| *ink == LogInk::Quiet && !text.is_empty())
        .expect("the log line paints a quiet timestamp");
    assert_eq!(stamp.0, "2023-11-14T22:13:20.000000Z");
    assert_eq!(
        channels(stamp.2),
        channels(palette.dim_color),
        "the timestamp is visually quieter than the body"
    );

    let keyword = runs
        .iter()
        .find(|(text, ink, _)| *ink == LogInk::Error && !text.is_empty())
        .expect("the error keyword is highlighted");
    assert_eq!(keyword.0, "ERROR");
    assert_eq!(
        channels(keyword.2),
        channels(palette.danger_color),
        "the error keyword wears the destructive red"
    );
}

// ---- scene-level fixtures -------------------------------------------------

/// The painted texts of a scene, for the panel-level assertions.
fn painted_scene_texts(scene: impl bevy::scene::Scene) -> Vec<String> {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    app.init_resource::<Assets<Font>>();
    let world = app.world_mut();
    let root = world
        .spawn_scene(scene)
        .expect("the scene resolves without app resources")
        .id();
    let texts: Vec<String> = world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect();
    assert!(world.despawn(root), "the scene despawns cleanly");
    texts
}

#[test]
fn the_dependency_sections_keep_their_honest_empty_dash() {
    let mut shell = ShellApp::new();
    let target = ServiceId::new("demo.service");
    let request = RequestId::new(1).expect("valid request id");
    shell.service_dependencies.begin(request, target.clone());
    let _ = shell
        .service_dependencies
        .resolve(request, target, relation_deps());
    let scene = super::dependencies_panel::service_dependencies_panel_scene_expanded(
        &shell,
        &palette(),
        &super::deps_tree::ServiceDepsExpansion::default(),
    );
    let texts = painted_scene_texts(scene);
    let dash = format!("{}: —", t("svc.wanted_by"));
    assert!(
        texts.iter().any(|text| text.contains(&dash)),
        "an untargeted relation keeps its honest dash ({dash:?}): {texts:?}"
    );
}
