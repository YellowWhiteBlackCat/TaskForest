//! Real-collector live smoke shared by every supported platform.
//!
//! Fixture tests prove parsers are correct; this module proves the wiring is
//! correct: the native composition edge spawns, every observation lane accepts
//! submissions, the event port drains typed outcomes, and live process rows
//! satisfy the host-neutral invariants from `taskmanager-platform-conformance`.

use std::time::Duration;

use taskmanager_application::{PlatformClient, RefreshRequest};
use taskmanager_platform_conformance::{
    assert_live_smoke_ok, assert_process_rows_consistent, collect_process_rows,
    drain_until_process_rows,
};
use taskmanager_platform_native::NativePlatformRuntime;

const DRAIN_DEADLINE: Duration = Duration::from_secs(5);
const DRAIN_POLL: Duration = Duration::from_millis(5);

#[cfg(target_os = "linux")]
const PROVIDER_PREFIX: &str = "linux.";
#[cfg(target_os = "windows")]
const PROVIDER_PREFIX: &str = "windows.";
#[cfg(target_os = "macos")]
const PROVIDER_PREFIX: &str = "macos.";

/// One full real-collector tick: spawn the native runtime, refresh every
/// observation lane, drain the event port, and assert only host-neutral
/// invariants. The same test runs on Linux, Windows, and macOS gates.
#[test]
fn live_smoke_native_runtime_publishes_host_neutral_outcomes() {
    let mut client = PlatformClient::new(
        NativePlatformRuntime::spawn().expect("native runtime must compose on this host"),
    );
    let submissions = client.request_refresh(RefreshRequest::All, 1);
    assert!(
        submissions.into_iter().all(|result| result.is_ok()),
        "every observation lane must accept submissions"
    );

    let drain = drain_until_process_rows(&mut client, DRAIN_DEADLINE, DRAIN_POLL)
        .expect("event port must stay live");
    assert_live_smoke_ok(&drain, PROVIDER_PREFIX).expect("live smoke contract failed");

    let rows = collect_process_rows(&drain);
    assert!(
        !rows.is_empty(),
        "live smoke must observe at least one process on this host"
    );
    assert_process_rows_consistent(&rows)
        .expect("live process rows must satisfy shared invariants");
}
