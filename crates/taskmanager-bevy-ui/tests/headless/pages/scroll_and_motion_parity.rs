//! test-intent: behavior
//!
//! The scroll assembly and the two owned micro-motions:
//!
//! - the **rail** (BEVY-175): every scroll area the frontend owns wears a
//!   visible translucent rail with a capsule thumb, and the rail is the
//!   scroll area's SIBLING — never a child of the scrolling content, which
//!   would scroll away with the rows;
//! - the **scroll owners** (BEVY-185 / BEVY-186): the sessions, startup and
//!   process-tree surfaces declare one, the last row stays reachable, and the
//!   last row keeps a bottom safety band;
//! - the **hot-plug slide** (BEVY-158): the storage device list item rides the
//!   shared typed expand progress, advanced per frame — never a sleep;
//! - the **charge flow** (BEVY-164): the battery icon's flow is a typed
//!   progress driven by the observed charge state, not by a timer.

use std::time::Duration;

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::Assets;
use bevy::camera::{Camera, Camera2d, ComputedCameraValues, RenderTargetInfo, Viewport};
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::query::With;
use bevy::ecs::world::World;
use bevy::image::TextureAtlasLayout;
use bevy::math::{UVec2, Vec2};
use bevy::picking::DefaultPickingPlugins;
use bevy::text::Font;
use bevy::time::TimeUpdateStrategy;
use bevy::transform::TransformPlugin;
use bevy::ui::{BackgroundColor, ComputedNode, Display, Node, UiGlobalTransform};
use bevy::ui_widgets::{ScrollArea, Scrollbar, ScrollbarThumb};
use bevy::window::{ExitCondition, PrimaryWindow, Window, WindowPlugin};
use taskforest_core::core::ScalarObservation;
use taskforest_core::core::device_state::DeviceState;
use taskforest_core::core::metrics::DiskMetrics;
use taskforest_core::core::power::{BatteryInfo, BatteryScalarObservations, PowerSupplySnapshot};
use taskforest_core::core::process::{
    ProcessApplicationIdentity, ProcessItem, ProcessMetadataObservation,
};
use taskforest_core::core::process_telemetry::{ContainerQuota, ContainerRollup, ContainerSummary};
use taskforest_core::core::session::{
    SessionControlAction, SessionFamilyFact, SessionIncarnation, SessionItem,
};
use taskforest_core::core::startup::{
    StartupControlPolicy, StartupEntry, StartupEntryId, StartupEntryLocator, StartupImpact,
    StartupImpactEvidence, StartupImpactUnknownReason, StartupScope, StartupSource,
};
use taskmanager_shell::ShellApp;
use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::UiSize;

use crate::app::{FrontendTrack, Page, PageContent, Route};
use crate::drain::ShellProjectionFolded;
use crate::pages::performance::device_curves::storage::{ExpandTarget, StorageExpand};
use crate::pages::performance::scene::battery::BatteryChargeFlow;
use crate::pages::process_tree::ProcessTreeViewport;
use crate::palette::ui_palette;
use crate::widgets::scrollbar::ProductScrollbar;
use crate::window::FrontendWindowPlugin;
use crate::window::tests::HeadlessFrontendPlugins;

/// The frame the headless composition lays out at.
const FRAME: (u32, u32) = (1280, 720);
/// The narrow frame the elastic-layout playbook asks every page to survive.
const NARROW: (u32, u32) = (720, 480);
/// The routes whose scroll areas this frontend owns.
const ROUTES: [Page; 9] = [
    Page::Processes,
    Page::Performance,
    Page::Services,
    Page::System,
    Page::Startup,
    Page::Sessions,
    Page::Containers,
    Page::Alerts,
    Page::Settings,
];

// ---- the headless composition ---------------------------------------------

fn headless_shell_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(HeadlessFrontendPlugins);
    app.add_plugins(FrontendWindowPlugin {
        runtime: crate::runtime::demo_platform_runtime(),
        palette: ui_palette(&Theme::dark(), UiSize::Standard),
    });
    app.init_resource::<Assets<Font>>();
    app.add_plugins((
        WindowPlugin {
            primary_window: None,
            exit_condition: ExitCondition::DontExit,
            ..Default::default()
        },
        DefaultPickingPlugins,
        TransformPlugin,
        bevy::text::TextPlugin,
        bevy::ui::UiPlugin,
    ));
    app.init_resource::<Assets<TextureAtlasLayout>>();
    // The manual clock makes every motion measurable: one frame is one
    // fixed step, so a typed progress is a number, not a race.
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        16,
    )));
    let (width, height) = FRAME;
    let size = UVec2::new(width, height);
    app.world_mut().spawn((
        Window {
            resolution: (width, height).into(),
            decorations: false,
            ..Default::default()
        },
        PrimaryWindow,
    ));
    app.world_mut().spawn((
        Camera2d,
        Camera {
            computed: ComputedCameraValues {
                target_info: Some(RenderTargetInfo {
                    physical_size: size,
                    scale_factor: 1.0,
                }),
                ..Default::default()
            },
            viewport: Some(Viewport {
                physical_size: size,
                ..Default::default()
            }),
            ..Default::default()
        },
    ));
    app
}

/// Mount one route and let its painters settle.
fn mount_page(app: &mut App, page: Page) {
    app.world_mut().resource_mut::<Route>().page = page;
    for _ in 0..4 {
        app.update();
    }
    assert_eq!(
        app.world_mut()
            .query_filtered::<&PageContent, ()>()
            .single(app.world())
            .expect("exactly one page content mounts")
            .page,
        page,
        "the page is mounted"
    );
}

/// Replace the shell the frontend reads.
fn set_shell(app: &mut App, shell: ShellApp) {
    app.world_mut().non_send_mut::<FrontendTrack>().shell = shell;
}

/// Whether `candidate` sits inside `ancestor`'s subtree.
fn is_descendant(world: &mut World, candidate: Entity, ancestor: Entity) -> bool {
    let mut current = candidate;
    for _ in 0..64 {
        let Some(parent) = world.get::<ChildOf>(current).map(|child| child.parent()) else {
            return false;
        };
        if parent == ancestor {
            return true;
        }
        current = parent;
    }
    false
}

/// The scroll areas the frontend does NOT own: the navigation strip
/// (`app.rs`) and the shared window-surface modal body
/// (`window_surface.rs`). Both carry their own scroll contract and are the
/// merge owner's to wire; this file proves every OTHER scroll area wears a
/// rail.
fn is_foreign_scroll_area(world: &mut World, area: Entity) -> bool {
    world
        .get::<crate::navigation::NavigationStrip>(area)
        .is_some()
        || world
            .get::<crate::window_surface::ModalBody>(area)
            .is_some()
}

/// The scroll areas a route owns.
fn owned_scroll_areas(world: &mut World) -> Vec<Entity> {
    let candidates: Vec<Entity> = world
        .query_filtered::<Entity, With<ScrollArea>>()
        .iter(world)
        .collect();
    candidates
        .into_iter()
        .filter(|area| !is_foreign_scroll_area(world, *area))
        .collect()
}

/// The rail that serves `area`: its SIBLING inside the same viewport wrapper.
fn rail_for(world: &mut World, area: Entity) -> Option<Entity> {
    let parent = world.get::<ChildOf>(area).map(|child| child.parent())?;
    world
        .get::<Children>(parent)
        .map(|children| {
            children
                .iter()
                .copied()
                .find(|sibling| world.get::<ProductScrollbar>(*sibling).is_some())
        })
        .unwrap_or(None)
}

/// Whether `entity` or any ancestor is `display: none`. A hidden branch owns
/// no reachable viewport: the compact Performance layout replaces its device
/// sidebar with the pill selector instead of squeezing the rail off-screen.
fn hidden_in_tree(world: &mut World, entity: Entity) -> bool {
    let mut current = entity;
    for _ in 0..64 {
        if world
            .get::<Node>(current)
            .is_some_and(|node| node.display == Display::None)
        {
            return true;
        }
        let Some(parent) = world.get::<ChildOf>(current).map(|child| child.parent()) else {
            return false;
        };
        current = parent;
    }
    false
}

/// The measured size of one node, in logical pixels.
fn measured_size(world: &mut World, entity: Entity) -> Vec2 {
    let node = world
        .get::<ComputedNode>(entity)
        .expect("the node is measured");
    node.size() * node.inverse_scale_factor()
}

/// The bottom edge of one node in window coordinates.
fn bottom_edge(world: &mut World, entity: Entity) -> f32 {
    let node = world
        .get::<ComputedNode>(entity)
        .expect("the node is measured");
    let transform = world
        .get::<UiGlobalTransform>(entity)
        .expect("the node is placed");
    transform.translation.y + node.size().y * node.inverse_scale_factor() / 2.0
}

/// The top edge of one node in window coordinates.
fn top_edge(world: &mut World, entity: Entity) -> f32 {
    let node = world
        .get::<ComputedNode>(entity)
        .expect("the node is measured");
    let transform = world
        .get::<UiGlobalTransform>(entity)
        .expect("the node is placed");
    transform.translation.y - node.size().y * node.inverse_scale_factor() / 2.0
}

// ---- BEVY-175: the rail ---------------------------------------------------

#[test]
fn every_owned_scroll_area_wears_a_visible_rail_sibling() {
    for page in ROUTES {
        let mut app = headless_shell_app();
        if page == Page::Containers {
            // The containers page renders its table only when the projection
            // names a container; an empty host renders the honest empty
            // branch, which owns no scroll area at all. The rail claim is
            // about the scroll area the page actually owns, so the fixture
            // gives it one.
            let mut shell = ShellApp::new();
            seed_container_rollup(&mut shell);
            set_shell(&mut app, shell);
        }
        mount_page(&mut app, page);
        let world = app.world_mut();
        let areas = owned_scroll_areas(world);
        assert!(
            !areas.is_empty(),
            "{page:?} owns at least one scroll area to rail"
        );
        for area in areas {
            let Some(rail) = rail_for(world, area) else {
                panic!("{page:?}: every scroll area owns a visible rail sibling");
            };
            assert!(
                !is_descendant(world, rail, area),
                "{page:?}: the rail is a sibling of the scroll area, never a child of the scrolling content"
            );
            let scrollbar = world
                .get::<Scrollbar>(rail)
                .expect("the rail carries the official scrollbar behavior");
            assert_eq!(
                scrollbar.target, area,
                "{page:?}: the rail drives the scroll area it belongs to"
            );
            let fill = world
                .get::<BackgroundColor>(rail)
                .expect("the rail paints a translucent track");
            assert!(
                fill.0.to_srgba().alpha > 0.0,
                "{page:?}: the rail is visible, not an invisible plate"
            );
            let thumb = world
                .get::<Children>(rail)
                .map(|children| {
                    children
                        .iter()
                        .any(|child| world.get::<ScrollbarThumb>(*child).is_some())
                })
                .unwrap_or(false);
            assert!(thumb, "{page:?}: the rail carries a capsule thumb");
        }
    }
}

#[test]
fn the_narrow_frame_keeps_every_owned_scroll_area_reachable() {
    let mut app = headless_shell_app();
    // The containers page paints a scroll owner only with a real container
    // fact; seed one so its narrow frame is measured like every other page.
    let mut shell = ShellApp::new();
    seed_container_rollup(&mut shell);
    set_shell(&mut app, shell);
    let (width, height) = NARROW;
    {
        let world = app.world_mut();
        let mut window = world
            .query_filtered::<&mut Window, With<PrimaryWindow>>()
            .single_mut(world)
            .expect("the primary window");
        window.resolution = (width, height).into();
    }
    for page in ROUTES {
        mount_page(&mut app, page);
        let world = app.world_mut();
        // A branch the responsive contract hides (the compact Performance
        // device sidebar) owns no reachable viewport; its content moved into
        // the pill selector. Every still-rendered scroll area must measure.
        let visible: Vec<Entity> = owned_scroll_areas(world)
            .into_iter()
            .filter(|area| !hidden_in_tree(world, *area))
            .collect();
        assert!(
            !visible.is_empty(),
            "{page:?} keeps a reachable scroll area at 720x480"
        );
        for area in visible {
            let size = measured_size(world, area);
            assert!(
                size.y > 0.0,
                "{page:?}: the scroll area keeps a positive viewport at 720x480: {size:?}"
            );
        }
    }
}

// ---- BEVY-185 / BEVY-186: the scroll owners -------------------------------

#[test]
fn the_sessions_page_declares_a_scroll_owner_with_a_bottom_safety_band() {
    let mut app = headless_shell_app();
    let mut shell = ShellApp::new();
    seed_projection_fact(
        &mut shell,
        ProjectionSeedFact::Sessions(Some(session_rows(24))),
    );
    set_shell(&mut app, shell);
    mount_page(&mut app, Page::Sessions);

    let world = app.world_mut();
    let owner = world
        .query_filtered::<Entity, (With<crate::pages::sessions::SessionsBody>, With<ScrollArea>)>()
        .single(world)
        .expect("the sessions page declares its scroll owner");
    let node = world
        .get::<ComputedNode>(owner)
        .expect("the scroll owner is measured");
    assert!(
        node.padding.max_inset.y > 0.0,
        "the last row keeps a bottom safety band instead of sitting flush against the viewport edge"
    );
    let content = node.content_size() * node.inverse_scale_factor();
    let visible = node.size() * node.inverse_scale_factor();
    assert!(
        content.y > visible.y,
        "the users inventory overflows its viewport, so the scroll owner is real"
    );

    // The last row is reachable: scroll to the end and read its bounds back.
    let last_row = world
        .query_filtered::<Entity, With<crate::pages::sessions::SessionsRowMarker>>()
        .iter(world)
        .last()
        .expect("the sessions table has rows");
    world
        .get_mut::<bevy::ui::ScrollPosition>(owner)
        .expect("the scroll owner carries a scroll position")
        .0 = Vec2::new(0.0, (content.y - visible.y).max(0.0));
    app.update();
    let world = app.world_mut();
    let viewport_bottom = top_edge(world, owner) + measured_size(world, owner).y;
    let row_bottom = bottom_edge(world, last_row);
    assert!(
        row_bottom <= viewport_bottom + 0.5,
        "the last row is inside the viewport once the owner is scrolled to its end"
    );
    assert!(
        row_bottom >= top_edge(world, owner) - 0.5,
        "the last row did not scroll past the viewport"
    );
}

#[test]
fn the_sessions_bubble_card_is_admitted_as_a_whole_group() {
    let mut app = headless_shell_app();
    let mut shell = ShellApp::new();
    let sessions = session_rows(24);
    seed_projection_fact(
        &mut shell,
        ProjectionSeedFact::Sessions(Some(sessions.clone())),
    );
    set_shell(&mut app, shell);
    mount_page(&mut app, Page::Sessions);

    // The gate is armed AFTER the composition started, exactly as the
    // product arms it: the page's own control freezes the target and asks
    // for a repaint. The shared modal owns the KEYBOARD path; the page's
    // bubble card is the surface that echoes the gate the page armed.
    let session = sessions
        .first()
        .cloned()
        .expect("the seeded inventory has a session");
    let armed = app
        .world_mut()
        .non_send_mut::<FrontendTrack>()
        .shell
        .select_session_control(&session, SessionControlAction::Disconnect);
    assert!(armed, "the shared gate arms the session control");
    app.world_mut()
        .trigger(crate::pages::sessions::paint::RepaintRequested);
    for _ in 0..2 {
        app.update();
    }

    let world = app.world_mut();
    let bubble = world
        .query_filtered::<Entity, With<crate::pages::sessions::bubble::SessionBubble>>()
        .single(world)
        .expect("the armed gate projects its bubble card");
    let height = measured_size(world, bubble).y;
    assert!(
        height > 0.0,
        "the bubble card is admitted as a whole group, not a clipped sliver"
    );
    // The bubble pushes the table down, but the scroll owner keeps the last
    // row reachable: the content grew past the viewport.
    let owner = world
        .query_filtered::<Entity, (With<crate::pages::sessions::SessionsBody>, With<ScrollArea>)>()
        .single(world)
        .expect("the sessions page declares its scroll owner");
    let node = world
        .get::<ComputedNode>(owner)
        .expect("the scroll owner is measured");
    let content = node.content_size() * node.inverse_scale_factor();
    let visible = node.size() * node.inverse_scale_factor();
    assert!(
        content.y > visible.y,
        "the bubble card pushes the table past the viewport, and the owner scrolls it"
    );
}

#[test]
fn the_startup_page_declares_a_scroll_owner_with_a_bottom_safety_band() {
    let mut app = headless_shell_app();
    let mut shell = ShellApp::new();
    seed_projection_fact(
        &mut shell,
        ProjectionSeedFact::StartupEntries(Some(startup_rows(24))),
    );
    set_shell(&mut app, shell);
    mount_page(&mut app, Page::Startup);

    let world = app.world_mut();
    let owner = world
        .query_filtered::<Entity, (With<crate::pages::startup::StartupBody>, With<ScrollArea>)>()
        .single(world)
        .expect("the startup page declares its scroll owner");
    let node = world
        .get::<ComputedNode>(owner)
        .expect("the scroll owner is measured");
    assert!(
        node.padding.max_inset.y > 0.0,
        "the last startup row keeps a bottom safety band"
    );
    let content = node.content_size() * node.inverse_scale_factor();
    let visible = node.size() * node.inverse_scale_factor();
    assert!(
        content.y > visible.y,
        "the startup inventory overflows its viewport, so the scroll owner is real"
    );
}

#[test]
fn the_process_tree_declares_a_bounded_scroll_owner() {
    let mut app = headless_shell_app();
    let mut shell = crate::demo_fixture::demo_shell();
    seed_tall_process_tree(&mut shell);
    set_shell(&mut app, shell);
    app.world_mut()
        .trigger(crate::window_surface::WindowSurfaceCommand::ProcessTree);
    for _ in 0..4 {
        app.update();
    }
    // The hierarchy is EXPANDED before the overflow is measured: a collapsed
    // tree is a single row and proves nothing about its scroll owner.
    app.world_mut()
        .trigger(crate::pages::process_tree::TreeExpansionCommand::ExpandAll);
    for _ in 0..4 {
        app.update();
    }
    let world = app.world_mut();
    let owner = world
        .query_filtered::<Entity, (With<ProcessTreeViewport>, With<ScrollArea>)>()
        .single(world)
        .expect("the process tree declares its own scroll owner");
    let modal_body = world
        .query_filtered::<Entity, With<crate::window_surface::ModalBody>>()
        .single(world)
        .expect("the shared modal body is mounted");
    let viewport = measured_size(world, owner).y;
    let modal = measured_size(world, modal_body).y;
    assert!(viewport > 0.0, "the tree's viewport is measured");
    assert!(
        viewport <= modal + 0.5,
        "the tree's viewport is bounded by the modal, not by its own content: {viewport} <= {modal}"
    );
    let node = world
        .get::<ComputedNode>(owner)
        .expect("the scroll owner is measured");
    let content = node.content_size() * node.inverse_scale_factor();
    let visible = node.size() * node.inverse_scale_factor();
    assert!(
        content.y > visible.y,
        "the hierarchy overflows its viewport, so the scroll owner is real: {content:?} > {visible:?}"
    );
}

/// A hierarchy taller than the modal's viewport: every worker owns its own
/// application identity, so the tree keeps one row per worker instead of
/// folding them into a single aggregate.
fn seed_tall_process_tree(shell: &mut ShellApp) {
    let mut processes = shell
        .projection()
        .processes
        .as_deref()
        .cloned()
        .unwrap_or_default();
    for offset in 0..40_u32 {
        let mut process = ProcessItem::new(92_000 + offset, format!("worker-{offset}"));
        let identity = ProcessApplicationIdentity::new(
            format!("org.taskforest.worker{offset}"),
            format!("Worker {offset}"),
            None,
        )
        .expect("the application identity is well formed");
        process.apply_application_identity(ProcessMetadataObservation::available(identity, 1));
        processes.push(process);
    }
    seed_projection_fact(shell, ProjectionSeedFact::Processes(Some(processes)));
}

// ---- BEVY-158: the hot-plug slide ----------------------------------------

#[test]
fn the_hot_plugged_storage_device_list_item_rides_the_shared_expand_progress() {
    let mut app = headless_shell_app();
    let mut shell = crate::demo_fixture::demo_shell();
    seed_hotplug_disk(&mut shell, true);
    set_shell(&mut app, shell);
    // The storage list lives behind the DISK device category: the user opens
    // the disk view first, exactly as the sidebar's device selector does.
    app.world_mut()
        .resource_mut::<crate::pages::performance::PerformanceDeviceFocus>()
        .0 = crate::pages::performance::PerformanceDeviceTarget::Disk(HOTPLUG_DISK.to_owned());
    mount_page(&mut app, Page::Performance);
    app.world_mut().trigger(ShellProjectionFolded);
    for _ in 0..2 {
        app.update();
    }

    let block = disk_block(&mut app, HOTPLUG_DISK);
    let start = app
        .world()
        .get::<StorageExpand>(block)
        .expect("the list item rides the shared expand motion")
        .progress;
    assert!(start < 1.0, "a hot-plugged device enters closed: {start}");

    // The motion is a typed per-frame progress, not a sleep: one frame moves
    // the number, and the number only ever grows towards one.
    app.update();
    let after = app
        .world()
        .get::<StorageExpand>(block)
        .expect("the list item still rides the motion")
        .progress;
    assert!(
        after > start,
        "the slide advances per frame: {start} -> {after}"
    );

    for _ in 0..60 {
        app.update();
    }
    let settled = app
        .world()
        .get::<StorageExpand>(block)
        .expect("the list item is still mounted")
        .progress;
    assert_eq!(settled, 1.0, "the list item settles fully open");

    // The device leaves: the item slides shut through the same typed state
    // and is retired once the slide reaches its end.
    seed_hotplug_disk(
        &mut app.world_mut().non_send_mut::<FrontendTrack>().shell,
        false,
    );
    app.world_mut().trigger(ShellProjectionFolded);
    app.update();
    let collapsing = app
        .world()
        .get::<StorageExpand>(block)
        .expect("the item is still mounted while it slides shut")
        .target;
    assert_eq!(
        collapsing,
        ExpandTarget::Collapsing,
        "the departure is a typed target flip, not an instant removal"
    );
    for _ in 0..60 {
        app.update();
    }
    assert!(
        app.world().get::<StorageExpand>(block).is_none(),
        "the slid-shut item is retired at the end of its frame"
    );
}

// ---- BEVY-164: the charge flow -------------------------------------------

#[test]
fn the_battery_icon_flow_follows_the_observed_charge_state() {
    let mut app = headless_shell_app();
    let mut shell = crate::demo_fixture::demo_shell();
    seed_battery(&mut shell, "Charging");
    set_shell(&mut app, shell);
    mount_page(&mut app, Page::Performance);
    app.world_mut().trigger(ShellProjectionFolded);
    for _ in 0..2 {
        app.update();
    }

    let flow = battery_flow(&mut app);
    assert!(
        app.world().get::<BatteryChargeFlow>(flow).is_some(),
        "the battery icon wears a charge flow"
    );
    let charging = app
        .world()
        .get::<BatteryChargeFlow>(flow)
        .expect("the flow is mounted")
        .charging;
    assert!(charging, "the observed charge state asks for the flow");

    let before = app
        .world()
        .get::<BatteryChargeFlow>(flow)
        .expect("the flow is mounted")
        .phase;
    app.update();
    let after = app
        .world()
        .get::<BatteryChargeFlow>(flow)
        .expect("the flow is mounted")
        .phase;
    assert!(
        after != before,
        "the flow advances per frame: {before} -> {after}"
    );

    // The pack stops charging: the flow stops with it. A timer would keep
    // the band travelling.
    seed_battery(
        &mut app.world_mut().non_send_mut::<FrontendTrack>().shell,
        "Discharging",
    );
    app.world_mut().trigger(ShellProjectionFolded);
    for _ in 0..2 {
        app.update();
    }
    let parked = app
        .world()
        .get::<BatteryChargeFlow>(flow)
        .expect("the flow is mounted")
        .phase;
    let still = app
        .world()
        .get::<BatteryChargeFlow>(flow)
        .expect("the flow is mounted")
        .charging;
    assert!(!still, "the observed charge state parked the flow");
    app.update();
    let after_park = app
        .world()
        .get::<BatteryChargeFlow>(flow)
        .expect("the flow is mounted")
        .phase;
    assert_eq!(
        parked, after_park,
        "the flow is driven by the observed charge state, not by a timer"
    );
}

// ---- fixtures -------------------------------------------------------------

/// The stable id of the hot-plugged disk the slide test drives.
const HOTPLUG_DISK: &str = "disk:hotplug:usb0";

/// Seed (or remove) the hot-plugged disk in the shell's snapshot.
fn seed_hotplug_disk(shell: &mut ShellApp, present: bool) {
    let Some(snapshot) = shell.projection().snapshot.clone() else {
        return;
    };
    let mut snapshot = snapshot;
    snapshot.disks.retain(|disk| disk.device_id != HOTPLUG_DISK);
    if present {
        let mut disk = DiskMetrics::new("/dev/sdb");
        disk.device_id = HOTPLUG_DISK.to_owned();
        disk.disk_type = "USB Drive".to_owned();
        disk.model = "TaskForest Flash".to_owned();
        snapshot.disks.push(disk);
    }
    seed_projection_fact(
        shell,
        ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
    );
}

/// Seed one healthy container rollup.
///
/// The containers page only paints its table (and therefore its scroll area)
/// when the projection names a container; the honest empty branch owns no
/// scroll area at all. The rail claim is about the scroll area the page
/// actually owns, so the fixture gives it one.
fn seed_container_rollup(shell: &mut ShellApp) {
    let summary = ContainerSummary {
        id: "c1".to_owned(),
        name: "taskforest-worker".to_owned(),
        runtime: None,
        cgroup_path: "/sys/fs/cgroup/c1".to_owned(),
        cpu_percentage: ScalarObservation::available(4.0, 1_000),
        memory_bytes: ScalarObservation::available(1024 * 1024, 1_000),
        member_pids: vec![1],
        quota: ContainerQuota::not_declared(),
    };
    seed_projection_fact(
        shell,
        ProjectionSeedFact::Containers(Some(ContainerRollup {
            state: DeviceState::healthy(1_000),
            containers: vec![summary],
        })),
    );
}

/// Seed the power supply with a battery in the given charge state.
fn seed_battery(shell: &mut ShellApp, status: &str) {
    let mut battery = BatteryInfo::new("BAT0", DeviceState::healthy(1_000));
    battery.model_name = "Primary Battery".to_owned();
    battery.status = status.to_owned();
    battery.apply_scalar_observations(BatteryScalarObservations {
        capacity_pct: ScalarObservation::available(78, 1_000),
        ..Default::default()
    });
    seed_projection_fact(
        shell,
        ProjectionSeedFact::PowerSupplies(Some(PowerSupplySnapshot {
            timestamp_ms: 1_000,
            batteries: vec![battery],
            ..Default::default()
        })),
    );
}

/// The mounted disk list item.
fn disk_block(app: &mut App, device_id: &str) -> Entity {
    app.world_mut()
        .query_filtered::<(Entity, &crate::pages::performance::DynBlock), ()>()
        .iter(app.world())
        .find(|(_, block)| {
            block.0 == crate::pages::performance::Section::Disk && block.1 == device_id
        })
        .map(|(entity, _)| entity)
        .expect("the disk list item is mounted")
}

/// The mounted battery charge flow.
fn battery_flow(app: &mut App) -> Entity {
    app.world_mut()
        .query_filtered::<Entity, With<BatteryChargeFlow>>()
        .iter(app.world())
        .next()
        .expect("the battery icon wears a charge flow")
}

/// A users inventory of `count` rows.
fn session_rows(count: usize) -> Vec<SessionItem> {
    (0..count)
        .map(|index| SessionItem {
            id: format!("session-{index}").into(),
            incarnation: Some(SessionIncarnation::LinuxLogind {
                boot_id: "test-boot-identity".into(),
                created_monotonic_usec: 700_000 + index as u64,
            }),
            uid: 1000 + index as u32,
            user: format!("user-{index}"),
            seat: Some(format!("seat{index}")),
            tty: Some(format!("tty{index}")),
            timestamp: Some("2026-10-12 09:30".to_owned()),
            family: SessionFamilyFact::default(),
        })
        .collect()
}

/// A startup inventory of `count` rows.
fn startup_rows(count: usize) -> Vec<StartupEntry> {
    (0..count)
        .map(|index| StartupEntry {
            id: StartupEntryId::new(format!("startup-{index}")),
            name: format!("unit-{index}"),
            enabled: index % 2 == 0,
            source: StartupSource::SystemService,
            scope: StartupScope::System,
            control_policy: StartupControlPolicy::default(),
            locator: StartupEntryLocator::new(format!("locator-{index}")),
            exec: format!("/usr/bin/unit-{index}"),
            impact: StartupImpact::Low,
            impact_evidence: StartupImpactEvidence::Unknown {
                reason: StartupImpactUnknownReason::NotInstrumented,
            },
        })
        .collect()
}
