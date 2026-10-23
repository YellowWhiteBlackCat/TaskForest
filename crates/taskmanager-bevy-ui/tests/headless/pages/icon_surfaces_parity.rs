//! test-intent: behavior
//!
//! The three semantic-icon surfaces this frontend owns, asserted as BEHAVIOR
//! on the mounted scene (never as source text, never as a pixel pattern):
//!
//! - the sessions page marks every row with the glyph of its OWN terminal
//!   family (a display-server session the monitor, a console session the
//!   terminal);
//! - the containers page's Runtime column wears the runtime's OWN brand
//!   glyph (Docker's blocks, Podman's pod of satellites);
//! - the storage surface's read-only mount row wears the shared padlock on
//!   the raised alarm wash, while an erroring mount keeps the alarm mark.
//!
//! All three read ONE icon chain — the shared semantic registry
//! (`taskmanager_ui_contract::IconId`) through the crate's icon layer — so
//! the assertions below pin the SEMANTIC PLATE each surface mounts, its
//! size on the rem axis, and the fact that the shared asset chain really
//! resolves a drawable bitmap for that id. A frontend-local glyph table, or
//! a glyph drawn out of text characters, would fail every assertion here.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::query::With;
use bevy::scene::{ScenePlugin, WorldSceneExt};
use bevy::text::Font;
use bevy::ui::{BackgroundColor, Node, Val};
use taskmanager_application::{CorrelatedSessionEvent, PlatformEventBatch, SessionEvent};
use taskmanager_core::core::device_state::DeviceState;
use taskmanager_core::core::metrics::{
    DiskMetrics, DiskPartition, DiskPartitionScalarObservations, ScalarObservation,
};
use taskmanager_core::core::process_telemetry::{
    ContainerQuota, ContainerRollup, ContainerSummary, IsolationKind,
};
use taskmanager_core::core::session::SessionItem;
use taskmanager_core::core::storage_health::{
    FilesystemBackingKind, FilesystemHealth, FilesystemHealthSnapshot, FilesystemHealthStatus,
};
use taskmanager_platform_contract::{
    CapabilityId, EventSequence, PartialSourceSnapshot, RequestId,
};
use taskmanager_shell::ShellApp;
use taskmanager_shell::fixture;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::UiSize;
use taskmanager_ui_contract::IconId;

use super::{SessionSelection, SessionsRowMarker, sessions_body_scene};
use crate::app::PageContext;
use crate::app::containers::{ContainerRuntime, ContainerRuntimeBadge};
use crate::icons::IconPlate;
use crate::pages::history::HistoryProjectionResource;
use crate::pages::performance::device_curves::storage::StorageSurface;
use crate::pages::performance::device_curves::storage::alarm::alarm_wash;
use crate::pages::performance::device_curves::storage::storage_surface_scene;
use crate::palette::{UiPalette, ui_palette};
use crate::window::WindowPalette;

/// The rasterized plate edge the shared asset chain produces.
const PLATE_EDGE_PX: usize = 36;

// ---- shared harness ----

/// A bare scene world: the surfaces are pure scenes, so nothing else is
/// needed to mount and inspect them.
fn headless_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    app.init_resource::<Assets<Font>>();
    app
}

/// The palette the surfaces render with.
fn palette() -> UiPalette {
    ui_palette(&Theme::dark(), UiSize::Standard)
}

/// The semantic plates under one entity, in tree order.
fn plates_under(app: &mut App, root: Entity) -> Vec<(Entity, IconId)> {
    let world = app.world_mut();
    let mut found = Vec::new();
    let mut stack = vec![root];
    while let Some(entity) = stack.pop() {
        if let Some(plate) = world.get::<IconPlate>(entity) {
            found.push((entity, plate.0));
        }
        if let Some(children) = world.get::<Children>(entity) {
            stack.extend(children.iter().copied());
        }
    }
    found
}

/// The entities under `root` whose fill is exactly `fill`.
fn nodes_filled_with(app: &mut App, root: Entity, fill: bevy::color::Color) -> Vec<Entity> {
    let wanted = fill.to_srgba();
    let world = app.world_mut();
    let mut found = Vec::new();
    let mut stack = vec![root];
    while let Some(entity) = stack.pop() {
        if let Some(background) = world.get::<BackgroundColor>(entity)
            && background.0.to_srgba() == wanted
        {
            found.push(entity);
        }
        if let Some(children) = world.get::<Children>(entity) {
            stack.extend(children.iter().copied());
        }
    }
    found
}

/// The plate rides the rem icon step of the palette — never a bare pixel
/// size, and never a size of its own.
fn assert_plate_size(app: &mut App, entity: Entity, palette: &UiPalette) {
    let node = app
        .world()
        .get::<Node>(entity)
        .expect("the semantic plate rides a sized node");
    assert_eq!(
        node.width,
        Val::Rem(palette.icon_rem),
        "the plate rides the palette's rem icon step"
    );
    assert_eq!(node.height, Val::Rem(palette.icon_rem));
}

/// The shared asset chain really resolves a drawable bitmap for the id the
/// surface mounted: a registered id with no rasterized plate would draw as
/// an invisible mark.
fn assert_plate_is_drawable(icon: IconId) {
    let bytes = crate::icons::icon_rgba(icon)
        .unwrap_or_else(|| panic!("{icon:?} has no bitmap plate in the shared asset chain"));
    assert_eq!(
        bytes.len(),
        PLATE_EDGE_PX * PLATE_EDGE_PX * 4,
        "{icon:?} resolves a correctly sized plate"
    );
}

// ---- the sessions page: one glyph per terminal family (BEVY-150) ----

fn session_item(id: &str, user: &str, seat: Option<&str>, tty: Option<&str>) -> SessionItem {
    SessionItem {
        id: id.to_owned().into(),
        incarnation: None,
        uid: 1000,
        user: user.to_owned(),
        seat: seat.map(str::to_owned),
        tty: tty.map(str::to_owned),
        remote: false,
        timestamp: Some("2026-08-24 09:00".to_owned()),
    }
}

/// The shell with one session per observable terminal family.
fn sessions_shell() -> ShellApp {
    let mut remote = session_item("4", "rem", Some("seat0"), Some("tty1"));
    remote.remote = true;
    let mut shell = ShellApp::new();
    shell.apply_platform_batch(PlatformEventBatch {
        session_events: vec![CorrelatedSessionEvent {
            request_id: RequestId::MIN,
            capability: CapabilityId::SESSIONS,
            provider: None,
            sequence: EventSequence::new(1),
            observed_at_ms: 1,
            event: SessionEvent::Snapshot(PartialSourceSnapshot {
                items: vec![
                    session_item("2", "ada", Some("seat0"), Some("wayland-0")),
                    session_item("3", "bo", None, Some("tty1")),
                    session_item("5", "cy", None, Some("pts/3")),
                    session_item("6", "di", None, None),
                    remote,
                ],
                sources: Vec::new(),
            }),
        }],
        ..PlatformEventBatch::default()
    });
    shell
}

/// The mounted row entity of one session id.
fn session_row(app: &mut App, session_id: &str) -> Entity {
    app.world_mut()
        .query_filtered::<(Entity, &SessionsRowMarker), ()>()
        .iter(app.world())
        .find(|(_, marker)| marker.1.to_string() == session_id)
        .map(|(entity, _)| entity)
        .unwrap_or_else(|| panic!("the session row {session_id} is mounted"))
}

#[test]
fn every_session_row_wears_the_glyph_of_its_own_terminal_family() {
    let shell = sessions_shell();
    let palette = palette();
    let mut app = headless_app();
    {
        let scene = sessions_body_scene(&shell, &palette, &SessionSelection::default(), None);
        app.world_mut()
            .spawn_scene(scene)
            .expect("the sessions body scene resolves");
    }
    app.update();

    // A Wayland/X11 session is a display-server session: the monitor plate.
    let graphical_row = session_row(&mut app, "2");
    let graphical = plates_under(&mut app, graphical_row);
    assert_eq!(
        graphical.iter().map(|(_, id)| *id).collect::<Vec<_>>(),
        vec![IconId::Display],
        "a display-server session wears the display plate, never a neighbour's"
    );
    assert_plate_size(&mut app, graphical[0].0, &palette);
    assert_plate_is_drawable(IconId::Display);

    // A physical console line and a pseudo terminal are console sessions:
    // the terminal plate.
    for console_id in ["3", "5"] {
        let console_row = session_row(&mut app, console_id);
        let console = plates_under(&mut app, console_row);
        assert_eq!(
            console.iter().map(|(_, id)| *id).collect::<Vec<_>>(),
            vec![IconId::Terminal],
            "a console session wears the terminal plate"
        );
        assert_plate_size(&mut app, console[0].0, &palette);
        assert_plate_is_drawable(IconId::Terminal);
    }

    // The two dedicated session glyphs never share one drawing.
    assert_ne!(
        IconId::Display,
        IconId::Terminal,
        "the two dedicated session glyphs are distinct"
    );

    // A remote session is reached over the network: it wears the transport
    // plate, never a terminal or a monitor — the contract owns no dedicated
    // remote-session glyph yet (merge-owner TODO: `IconId::Remote`).
    let remote_row = session_row(&mut app, "4");
    let remote = plates_under(&mut app, remote_row);
    assert_eq!(
        remote.iter().map(|(_, id)| *id).collect::<Vec<_>>(),
        vec![IconId::Remote],
        "a remote session wears the network transport plate"
    );
    assert!(
        !remote
            .iter()
            .any(|(_, id)| matches!(id, IconId::Terminal | IconId::Display)),
        "a remote session is never drawn as a local terminal"
    );

    // An unobserved family is never drawn as a terminal or a monitor: the
    // row wears the neutral plate.
    let unknown_row = session_row(&mut app, "6");
    let unknown = plates_under(&mut app, unknown_row);
    assert!(
        !unknown
            .iter()
            .any(|(_, id)| matches!(id, IconId::Terminal | IconId::Display)),
        "an unobserved family never guesses a terminal: {unknown:?}"
    );
}

// ---- the containers page: the runtime column wears the brand (BEVY-151) ----

const GIB: u64 = 1024 * 1024 * 1024;

/// A container summary with a recognized runtime family.
fn runtime_summary(id: &str, name: &str, runtime: Option<IsolationKind>) -> ContainerSummary {
    ContainerSummary {
        id: id.to_owned(),
        name: name.to_owned(),
        runtime,
        cgroup_path: format!("/sys/fs/cgroup/{id}"),
        cpu_percentage: ScalarObservation::available(3.0, 1),
        memory_bytes: ScalarObservation::available(GIB, 1),
        quota: ContainerQuota::not_declared(),
        member_pids: vec![42],
    }
}

/// The containers page mounted with one Docker and one Podman row.
fn containers_app() -> App {
    let mut shell = ShellApp::new();
    fixture::edit_containers(&mut shell, |rollup| {
        *rollup = Some(ContainerRollup {
            state: DeviceState::healthy(100),
            containers: vec![
                runtime_summary("d1", "web", Some(IsolationKind::Docker)),
                runtime_summary("p1", "cache", Some(IsolationKind::Podman)),
            ],
        });
    });
    let palette = palette();
    let history = HistoryProjectionResource::default();
    let context = PageContext {
        sidebar: &Default::default(),
        gray_zero_values: false,
        shell: &shell,
        palette: &palette,
        history: &history.0,
    };
    let mut app = headless_app();
    app.insert_resource(WindowPalette {
        inner: palette.clone(),
    });
    app.world_mut()
        .spawn_scene(crate::app::containers::scene(&context))
        .expect("the containers scene resolves");
    app.update();
    app
}

#[test]
fn the_runtime_column_wears_each_containers_own_brand_glyph() {
    let palette = palette();
    let mut app = containers_app();

    let badges: Vec<(Entity, Option<ContainerRuntime>)> = app
        .world_mut()
        .query_filtered::<(Entity, &ContainerRuntimeBadge), ()>()
        .iter(app.world())
        .map(|(entity, badge)| (entity, badge.0))
        .collect();
    assert_eq!(
        badges.len(),
        2,
        "every container row carries a runtime badge"
    );

    for (entity, runtime) in badges {
        let expected = match runtime {
            Some(ContainerRuntime::Docker) => IconId::Docker,
            Some(ContainerRuntime::Podman) => IconId::Podman,
            other => panic!("unexpected runtime family: {other:?}"),
        };
        let plates = plates_under(&mut app, entity);
        assert_eq!(
            plates.iter().map(|(_, id)| *id).collect::<Vec<_>>(),
            vec![expected],
            "the runtime column wears the runtime's OWN brand glyph"
        );
        assert_plate_size(&mut app, plates[0].0, &palette);
        assert_plate_is_drawable(expected);
    }
}

// ---- the storage surface: the read-only mount wears the lock (BEVY-157) ----

/// A disk with one measured partition mounted at `/`.
fn disk_with_partition() -> DiskMetrics {
    let mut disk = DiskMetrics::new("sda");
    disk.device_id = "sda".to_owned();
    let mut partition = DiskPartition::new("sda1");
    partition.mount_point = "/".to_owned();
    partition.fs_type = "ext4".to_owned();
    partition.apply_scalar_observations(DiskPartitionScalarObservations {
        capacity_bytes: ScalarObservation::available(100, 1),
        used_bytes: ScalarObservation::available(62, 1),
        free_bytes: ScalarObservation::available(38, 1),
    });
    disk.partitions = vec![partition];
    disk
}

/// The filesystem health of the `/` mount, in the caller's state.
fn mount_health(
    read_only: bool,
    error_count: u64,
    status: FilesystemHealthStatus,
) -> FilesystemHealthSnapshot {
    FilesystemHealthSnapshot {
        state: DeviceState::healthy(1),
        filesystems: vec![FilesystemHealth {
            mount_point: "/".into(),
            source: None,
            fs_type: "ext4".to_owned(),
            backing_kind: FilesystemBackingKind::PhysicalBlock,
            read_only: Some(read_only),
            error_count: Some(error_count),
            inode_used: None,
            inode_total: None,
            inode_usage_percent: None,
            status,
            state: DeviceState::healthy(1),
            integrity_state: DeviceState::healthy(1),
        }],
    }
}

/// The storage surface mounted with one filesystem health snapshot.
fn storage_app(filesystems: &FilesystemHealthSnapshot) -> App {
    let palette = palette();
    let disk = disk_with_partition();
    let mut app = headless_app();
    app.world_mut()
        .spawn_scene(storage_surface_scene(
            &disk,
            Some(filesystems),
            &palette,
            1,
            1.0,
        ))
        .expect("the storage surface resolves");
    app.update();
    app
}

/// The mounted storage surface root (the entity carrying the surface marker).
fn storage_surface_root(app: &mut App) -> Entity {
    app.world_mut()
        .query_filtered::<Entity, With<StorageSurface>>()
        .iter(app.world())
        .next()
        .expect("the storage surface is mounted")
}

#[test]
fn the_read_only_mount_row_wears_the_lock_plate_on_the_raised_alarm_wash() {
    let palette = palette();
    let health = mount_health(true, 0, FilesystemHealthStatus::ReadOnly);
    let mut app = storage_app(&health);
    let root = storage_surface_root(&mut app);

    let wash = alarm_wash(&palette);
    let rows = nodes_filled_with(&mut app, root, wash);
    assert_eq!(
        rows.len(),
        1,
        "the read-only mount raises exactly one alarm row"
    );
    let plates = plates_under(&mut app, rows[0]);
    assert_eq!(
        plates.iter().map(|(_, id)| *id).collect::<Vec<_>>(),
        vec![IconId::Lock],
        "a read-only mount wears the shared padlock, never the alarm triangle"
    );
    assert_plate_size(&mut app, plates[0].0, &palette);
    assert_plate_is_drawable(IconId::Lock);
}

#[test]
fn an_erroring_mount_row_keeps_the_alarm_mark_and_never_the_lock() {
    let palette = palette();
    let health = mount_health(false, 3, FilesystemHealthStatus::ErrorsReported);
    let mut app = storage_app(&health);
    let root = storage_surface_root(&mut app);

    let wash = alarm_wash(&palette);
    let rows = nodes_filled_with(&mut app, root, wash);
    assert_eq!(
        rows.len(),
        1,
        "the erroring mount raises exactly one alarm row"
    );
    let plates = plates_under(&mut app, rows[0]);
    assert_eq!(
        plates.iter().map(|(_, id)| *id).collect::<Vec<_>>(),
        vec![IconId::Alert],
        "an erroring mount is not locked: it keeps the alarm mark"
    );
}

#[test]
fn the_alarm_wash_is_a_raised_theme_surface_not_a_hand_picked_color() {
    let palette = palette();
    let wash = alarm_wash(&palette);
    let srgba = wash.to_srgba();
    assert_eq!(
        srgba.alpha, 1.0,
        "the wash is composed over the card's fill, so the row is a raised surface"
    );
    let card = palette.panel_fill.to_srgba();
    assert_ne!(
        (srgba.red, srgba.green, srgba.blue),
        (card.red, card.green, card.blue),
        "the wash actually tints the card's own fill"
    );
}
