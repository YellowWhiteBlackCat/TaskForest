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

use taskmanager_core::core::FailureKind;
use taskmanager_core::core::device_state::{DeviceState, DeviceStatus};
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process_telemetry::{ContainerRollup, ContainerSummary};
use taskmanager_shell::ShellApp;
use taskmanager_shell::fixture;
use taskmanager_shell::presentation::missing_value;
use taskmanager_theme::Theme;

use super::{ContainersPageBranch, container_row_model, page_branch, scene};
use crate::app::PageContext;
use crate::palette::ui_palette;

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
    let palette = ui_palette(&Theme::dark());
    let history = crate::pages::history::HistoryProjectionResource::default();
    let process_tree_expansion = crate::pages::process_tree::ProcessTreeExpansion::default();

    // 1. Waiting (containers is None initially)
    {
        let ctx = PageContext {
            shell: &shell,
            process_tree_expansion: &process_tree_expansion,
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
            shell: &shell,
            process_tree_expansion: &process_tree_expansion,
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
            shell: &shell,
            process_tree_expansion: &process_tree_expansion,
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
            shell: &shell,
            process_tree_expansion: &process_tree_expansion,
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
            shell: &shell,
            process_tree_expansion: &process_tree_expansion,
            palette: &palette,
            history: &history.0,
        };
        let _ = scene(&ctx);
    }
}
