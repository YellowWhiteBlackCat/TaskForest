//! test-intent: behavior
//! Root-level GPUI interaction suites that exercise cross-crate surface,
//! confirmation, and navigation behavior. Renderer-internal projections and
//! render-only smoke coverage live in `taskmanager-gpui`'s own test suite.

use taskmanager_core::core::process::ProcessItem;

#[path = "gpui_behavior/confirmations.rs"]
mod confirmations;
#[path = "gpui_behavior/nav_chrome.rs"]
mod nav_chrome;
#[path = "gpui_behavior/render_coverage.rs"]
mod render_coverage;

fn proc(pid: u32, name: &str) -> ProcessItem {
    taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(pid)
        .parent_pid(None)
        .name(name.into())
        .cmdline(String::new())
        .scalar_observations(taskmanager_core::core::process::ProcessScalarObservations {
            start_token: taskmanager_core::core::ScalarObservation::available(
                u64::from(pid) + 1_000,
                1,
            ),
            ..Default::default()
        })
        .current_cpu_percentage(0.0)
        .current_memory_bytes(0)
        .current_disk_read_bytes_per_sec(0)
        .current_disk_write_bytes_per_sec(0)
        .status("R".into())
        .metadata_observations(
            taskmanager_core::core::process::ProcessMetadataObservations::current(
                taskmanager_core::core::process::ProcessOwner::opaque("u"),
                None,
                1,
            ),
        )
        .build()
}
