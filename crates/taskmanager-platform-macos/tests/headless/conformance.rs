//! Shared platform-conformance scenarios run on the real macOS host.

use std::time::Duration;

use taskmanager_application::{PlatformClient, RefreshRequest};
use taskmanager_platform_conformance::{
    assert_live_smoke_ok, assert_process_rows_consistent, collect_process_rows,
    drain_until_process_rows,
};
use taskmanager_platform_macos::MacOsPlatformRuntime;

const DRAIN_DEADLINE: Duration = Duration::from_secs(5);
const DRAIN_POLL: Duration = Duration::from_millis(5);

/// The macOS adapter must publish host-neutral typed outcomes from the real
/// host: every observation lane accepts submissions, the event port drains,
/// failures are attributed to `macos.*`, and live process rows satisfy the
/// shared invariants.
#[test]
fn live_runtime_conformance_publishes_host_neutral_typed_outcomes() {
    let mut client =
        PlatformClient::new(MacOsPlatformRuntime::spawn().expect("complete macOS composition"));
    let submissions = client.request_refresh(RefreshRequest::All, 1);
    assert!(
        submissions.into_iter().all(|result| result.is_ok()),
        "every observation lane must accept submissions"
    );

    let drain = drain_until_process_rows(&mut client, DRAIN_DEADLINE, DRAIN_POLL)
        .expect("event port must stay live");
    assert_live_smoke_ok(&drain, "macos.").expect("macOS live smoke contract failed");

    let rows = collect_process_rows(&drain);
    assert!(
        !rows.is_empty(),
        "macOS live smoke must observe at least one process"
    );
    assert_process_rows_consistent(&rows)
        .expect("macOS live process rows must satisfy shared invariants");
}
