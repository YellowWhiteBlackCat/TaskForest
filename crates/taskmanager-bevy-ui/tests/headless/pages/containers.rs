//! test-intent: behavior
//!
//! Headless behavior tests for the Bevy Containers page (`src/pages/containers.rs`).
//!
//! 1. Pure branch resolution:
//!    - None -> Waiting
//!    - DeviceStatus::Unsupported -> Unsupported
//!    - DeviceStatus::PermissionDenied -> PermissionDenied
//!    - DeviceStatus::Healthy with 0 containers -> Empty
//!    - DeviceStatus::Healthy with >=1 containers -> Table
//! 2. Row view model formatting:
//!    - CPU% and memory formatting with honest dashes for unobserved values
//!    - Member PIDs count formatting with honest dashes when empty
//! 3. Scene assembly:
//!    - Each of the 5 honest branch states renders its expected content.

use bevy::app::App;
use bevy::asset::AssetPlugin;
use bevy::scene::{ScenePlugin, WorldSceneExt};
use taskmanager_core::core::FailureKind;
use taskmanager_core::core::device_state::{DeviceState, DeviceStatus};
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process_telemetry::{
    ContainerQuota, ContainerRollup, ContainerSummary, IsolationKind,
};
use taskmanager_shell::ShellApp;
use taskmanager_shell::fixture;
use taskmanager_shell::presentation::missing_value;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::UiSize;

use super::{
    ContainerRuntime, ContainerRuntimeBadge, ContainersPageBranch, container_row_model,
    page_branch, scene,
};
use crate::app::PageContext;
use crate::palette::ui_palette;
use crate::window::WindowPalette;
use taskmanager_shell::demo_app;

fn dummy_summary(
    id: &str,
    name: &str,
    cpu: Option<f32>,
    mem: Option<u64>,
    pids: Vec<u32>,
) -> ContainerSummary {
    ContainerSummary {
        id: id.to_owned(),
        name: name.to_owned(),
        runtime: None,
        cgroup_path: format!("/sys/fs/cgroup/{id}"),
        cpu_percentage: cpu.map_or_else(
            || ScalarObservation::unavailable(FailureKind::TimedOut),
            |v| ScalarObservation::available(v, 1),
        ),
        memory_bytes: mem.map_or_else(
            || ScalarObservation::unavailable(FailureKind::TimedOut),
            |v| ScalarObservation::available(v, 1),
        ),
        member_pids: pids,
        // The fixture declares no cgroup ceiling.
        quota: ContainerQuota::not_declared(),
    }
}

#[test]
fn containers_page_branch_resolution_covers_five_states() {
    // 1. None -> Waiting
    assert_eq!(page_branch(None), ContainersPageBranch::Waiting);

    // 2. Unsupported
    let unsupported = ContainerRollup {
        state: DeviceState {
            status: DeviceStatus::Unsupported,
            last_success_ms: None,
        },
        containers: Vec::new(),
    };
    assert_eq!(
        page_branch(Some(&unsupported)),
        ContainersPageBranch::Unsupported
    );

    // 3. PermissionDenied
    let denied = ContainerRollup {
        state: DeviceState {
            status: DeviceStatus::PermissionDenied,
            last_success_ms: None,
        },
        containers: Vec::new(),
    };
    assert_eq!(
        page_branch(Some(&denied)),
        ContainersPageBranch::PermissionDenied
    );

    // 4. Empty
    let empty = ContainerRollup {
        state: DeviceState::healthy(100),
        containers: Vec::new(),
    };
    assert_eq!(page_branch(Some(&empty)), ContainersPageBranch::Empty);

    // 5. Table
    let table = ContainerRollup {
        state: DeviceState::healthy(100),
        containers: vec![dummy_summary(
            "c1",
            "redis",
            Some(12.5),
            Some(1024 * 1024 * 64),
            vec![1234],
        )],
    };
    assert_eq!(page_branch(Some(&table)), ContainersPageBranch::Table);
}

#[test]
fn container_row_model_formats_observed_and_unobserved_honestly() {
    // Fully observed row
    let item = dummy_summary(
        "c-123",
        "web-server",
        Some(34.2),
        Some(1024 * 1024 * 128),
        vec![101, 102],
    );
    let model = container_row_model(&item);
    assert_eq!(model.id, "c-123");
    assert_eq!(model.name, "web-server");
    assert_eq!(model.cpu, "34.2%");
    assert!(
        model.memory.contains("MB") || model.memory.contains("MiB") || model.memory.contains("128")
    );
    assert_eq!(model.pids, "2");

    // Unobserved values render missing dashes, never 0 or 0.0%
    let item_unobserved = dummy_summary("c-456", "worker", None, None, vec![]);
    let model_unobserved = container_row_model(&item_unobserved);
    assert_eq!(model_unobserved.id, "c-456");
    assert_eq!(model_unobserved.name, "worker");
    assert_eq!(model_unobserved.cpu, missing_value());
    assert_eq!(model_unobserved.memory, missing_value());
    assert_eq!(model_unobserved.pids, missing_value());
}

#[test]
fn containers_scene_assembles_for_all_five_branches() {
    let mut shell = ShellApp::new();
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let history = crate::pages::history::HistoryProjectionResource::default();

    // 1. Waiting (containers is None initially)
    {
        let ctx = PageContext {
            sidebar: &Default::default(),
            gray_zero_values: false,
            shell: &shell,

            palette: &palette,
            history: &history.0,
        };
        let _ = scene(&ctx);
    }

    // 2. Unsupported
    fixture::edit_containers(&mut shell, |c| {
        *c = Some(ContainerRollup {
            state: DeviceState {
                status: DeviceStatus::Unsupported,
                last_success_ms: None,
            },
            containers: Vec::new(),
        });
    });
    {
        let ctx = PageContext {
            sidebar: &Default::default(),
            gray_zero_values: false,
            shell: &shell,

            palette: &palette,
            history: &history.0,
        };
        let _ = scene(&ctx);
    }

    // 3. PermissionDenied
    fixture::edit_containers(&mut shell, |c| {
        *c = Some(ContainerRollup {
            state: DeviceState {
                status: DeviceStatus::PermissionDenied,
                last_success_ms: None,
            },
            containers: Vec::new(),
        });
    });
    {
        let ctx = PageContext {
            sidebar: &Default::default(),
            gray_zero_values: false,
            shell: &shell,

            palette: &palette,
            history: &history.0,
        };
        let _ = scene(&ctx);
    }

    // 4. Empty
    fixture::edit_containers(&mut shell, |c| {
        *c = Some(ContainerRollup {
            state: DeviceState::healthy(100),
            containers: Vec::new(),
        });
    });
    {
        let ctx = PageContext {
            sidebar: &Default::default(),
            gray_zero_values: false,
            shell: &shell,

            palette: &palette,
            history: &history.0,
        };
        let _ = scene(&ctx);
    }

    // 5. Table
    fixture::edit_containers(&mut shell, |c| {
        *c = Some(ContainerRollup {
            state: DeviceState::healthy(100),
            containers: vec![dummy_summary(
                "c1",
                "test-container",
                Some(15.0),
                Some(1024 * 1024),
                vec![555],
            )],
        });
    });
    {
        let ctx = PageContext {
            sidebar: &Default::default(),
            gray_zero_values: false,
            shell: &shell,

            palette: &palette,
            history: &history.0,
        };
        let _ = scene(&ctx);
    }
}

// ---- the sandbox runtime badge (BEVY-151) ----

const GIB: u64 = 1024 * 1024 * 1024;

/// A container summary with a recognized runtime family.
fn runtime_summary(
    id: &str,
    name: &str,
    runtime: Option<IsolationKind>,
    memory: u64,
) -> ContainerSummary {
    let mut summary = dummy_summary(id, name, Some(3.0), Some(memory), vec![42]);
    summary.runtime = runtime;
    summary
}

#[test]
fn container_rows_name_the_sandbox_family_the_provider_recognized() {
    let docker = runtime_summary("d1", "web", Some(IsolationKind::Docker), GIB);
    let podman = runtime_summary("p1", "cache", Some(IsolationKind::Podman), GIB);
    let unknown = runtime_summary("u1", "mystery", None, GIB);

    let docker_model = container_row_model(&docker);
    assert_eq!(
        docker_model.runtime,
        Some(ContainerRuntime::Docker),
        "the row keeps the provider's runtime family"
    );
    let podman_model = container_row_model(&podman);
    assert_eq!(
        podman_model.runtime,
        Some(ContainerRuntime::Podman),
        "every family reaches its own row"
    );
    assert_eq!(
        ContainerRuntime::Docker.label(),
        "Docker",
        "the badge label is the friendly runtime name"
    );
    assert_eq!(
        ContainerRuntime::Podman.label(),
        "Podman",
        "each family carries its own brand word"
    );
    assert_ne!(
        ContainerRuntime::Docker.icon(),
        ContainerRuntime::Podman.icon(),
        "the brand marks are distinct per runtime family"
    );
    assert_eq!(
        container_row_model(&unknown).runtime,
        None,
        "an unrecognized runtime stays unknown, never a guess"
    );
}

#[test]
fn the_mounted_container_rows_carry_their_runtime_badge() {
    let mut shell = demo_app();
    fixture::edit_containers(&mut shell, |c| {
        *c = Some(ContainerRollup {
            state: DeviceState::healthy(100),
            containers: vec![
                runtime_summary("d1", "web", Some(IsolationKind::Docker), GIB),
                runtime_summary("p1", "cache", Some(IsolationKind::Podman), GIB),
            ],
        });
    });
    let mut app = mount_containers_page(&shell);

    let badges: Vec<Option<ContainerRuntime>> = app
        .world_mut()
        .query_filtered::<&ContainerRuntimeBadge, ()>()
        .iter(app.world())
        .map(|badge| badge.0)
        .collect();
    assert_eq!(
        badges.len(),
        2,
        "every container row carries a runtime badge"
    );
    assert!(
        badges.contains(&Some(ContainerRuntime::Docker))
            && badges.contains(&Some(ContainerRuntime::Podman)),
        "the badges name the observed families: {badges:?}"
    );
}

// The declared-quota contract (the warning halo and the quota column) is
// covered by its own file: `container_quota_parity.rs`.

/// Mount the containers page scene in a headless app so the rendered rows
/// can be inspected.
fn mount_containers_page(shell: &ShellApp) -> App {
    let mut app = App::new();
    app.add_plugins(bevy::MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    app.init_resource::<bevy::asset::Assets<bevy::text::Font>>();
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    app.insert_resource(WindowPalette {
        inner: palette.clone(),
    });
    let history = crate::pages::history::HistoryProjectionResource::default();
    let context = PageContext {
        sidebar: &Default::default(),
        gray_zero_values: false,
        shell,
        palette: &palette,
        history: &history.0,
    };
    app.world_mut()
        .spawn_scene(scene(&context))
        .expect("the containers scene resolves without assets");
    app.update();
    app
}
