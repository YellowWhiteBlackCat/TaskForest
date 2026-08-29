// test-intent: behavior
//! Headless behavior tests for the Containers page upgrade: the typed
//! page-branch routing (source states never masquerade as an empty host),
//! the honest row fold, and the live open/dismiss lifecycle of the page
//! surface.

use super::*;
use taskmanager_core::core::device_state::{DeviceState, DeviceStatus};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process_telemetry::{ContainerSummary, IsolationKind};

fn populated_rollup() -> ContainerRollup {
    let mut rollup = ContainerRollup::empty_healthy(1_000);
    rollup.containers.push(ContainerSummary {
        id: "/docker/abc".into(),
        name: "abc".into(),
        runtime: Some(IsolationKind::Docker),
        cgroup_path: "/docker/abc".into(),
        cpu_percentage: ScalarObservation::available(120.5, 1_000),
        memory_bytes: ScalarObservation::available(3 * 1024 * 1024, 1_000),
        member_pids: vec![100, 101, 102],
    });
    rollup
}

fn unavailable_rollup(status: DeviceStatus) -> ContainerRollup {
    ContainerRollup::unavailable(DeviceState {
        status,
        last_success_ms: None,
    })
}

#[test]
fn page_branch_separates_typed_source_states_from_a_container_free_host() {
    assert_eq!(page_branch(None), ContainersPageBranch::Waiting);
    assert_eq!(
        page_branch(Some(&unavailable_rollup(DeviceStatus::Unsupported))),
        ContainersPageBranch::Unsupported
    );
    assert_eq!(
        page_branch(Some(&unavailable_rollup(DeviceStatus::PermissionDenied))),
        ContainersPageBranch::PermissionDenied
    );
    assert_eq!(
        page_branch(Some(&unavailable_rollup(DeviceStatus::Stale))),
        ContainersPageBranch::Stale
    );
    // A healthy container-free host is a real state, distinct from every
    // failed source above.
    let empty = ContainerRollup::empty_healthy(1_000);
    assert_eq!(
        page_branch(Some(&empty)),
        ContainersPageBranch::Empty,
        "a healthy host with zero containers must not read as a failure"
    );
    assert_eq!(
        page_branch(Some(&populated_rollup())),
        ContainersPageBranch::Table
    );
}

#[test]
fn row_vm_folds_scalar_availability_honestly() {
    let mut rollup = populated_rollup();

    // A partial reading is still a reading.
    rollup.containers[0].cpu_percentage =
        ScalarObservation::partial(42.0, 1_000, FailureKind::TimedOut);
    assert_eq!(
        container_row_vm(&rollup.containers[0]).cpu,
        "42.0%",
        "a partial observation renders its value, not a failure dash"
    );

    // Unavailable scalars render the shared dash — never a fabricated zero.
    rollup.containers[0].cpu_percentage = ScalarObservation::unavailable(FailureKind::TimedOut);
    rollup.containers[0].memory_bytes =
        ScalarObservation::unavailable(FailureKind::PermissionDenied);
    rollup.containers[0].member_pids.clear();
    let vm = container_row_vm(&rollup.containers[0]);
    assert_eq!(vm.cpu, "—");
    assert_eq!(vm.memory, "—");
    assert_eq!(vm.processes, "—");
    assert_eq!(vm.runtime, "Docker");
    assert_eq!(vm.name, "abc");

    // A populated membership renders its count.
    rollup.containers[0].member_pids = vec![7, 8];
    assert_eq!(container_row_vm(&rollup.containers[0]).processes, "2");
}

#[test]
fn opening_the_surface_renders_the_page_and_dismissal_closes_it() {
    let mut app = crate::IcedApp::demo();

    let _ = app.update(Message::OpenContainers);
    assert_eq!(
        app.local_surface(),
        Some(&crate::app::LocalSurface::Containers),
        "the toolbar trigger opens the containers page surface"
    );
    // The page renders headless on the honest branch the demo projection
    // provides (waiting until a rollup arrives); the element borrows the
    // app, so the render scope ends before the dismissal mutates it.
    {
        let _page = render(&app);
    }

    // Escape / the header close both publish DismissOverlay: the page closes
    // through the same live slot with no other side effect.
    let _ = app.update(Message::DismissOverlay);
    assert_eq!(
        app.local_surface(),
        None,
        "dismissal closes the containers page"
    );
}

#[test]
fn page_body_renders_every_typed_branch_without_panic() {
    let app = crate::IcedApp::demo();
    let theme_snapshot = app.theme();

    let rollups = [
        None,
        Some(unavailable_rollup(DeviceStatus::Unsupported)),
        Some(unavailable_rollup(DeviceStatus::PermissionDenied)),
        Some(unavailable_rollup(DeviceStatus::Stale)),
        Some(ContainerRollup::empty_healthy(1_000)),
        Some(populated_rollup()),
    ];
    for rollup in &rollups {
        let _ = page_body(theme_snapshot, rollup.as_ref());
    }
}
