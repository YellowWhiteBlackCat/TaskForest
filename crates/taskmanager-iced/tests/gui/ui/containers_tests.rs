use super::*;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::process_telemetry::ContainerSummary;

fn healthy_rollup() -> ContainerRollup {
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

#[test]
fn container_rows_render_typed_values_and_no_fabricated_zeros() {
    let rollup = healthy_rollup();
    let row = &rollup.containers[0];
    assert_eq!(runtime_label(row.runtime.as_ref()), "Docker");
    assert_eq!(
        scalar_text(row.cpu_percentage, |value| format!("{value:.1}%")),
        "120.5%"
    );
    assert_eq!(scalar_text(row.memory_bytes, bytes), "3.0 MiB");
    assert_eq!(row.member_pids.len(), 3);
}

#[test]
fn unavailable_scalars_render_dash_not_zero() {
    let mut rollup = healthy_rollup();
    rollup.containers[0].cpu_percentage =
        ScalarObservation::unavailable(FailureKind::PermissionDenied);
    rollup.containers[0].memory_bytes =
        ScalarObservation::unavailable(FailureKind::PermissionDenied);
    assert_eq!(
        scalar_text(rollup.containers[0].cpu_percentage, |value| format!(
            "{value:.1}%"
        )),
        "—"
    );
    assert_eq!(scalar_text(rollup.containers[0].memory_bytes, bytes), "—");
}

#[test]
fn healthy_empty_rollup_is_not_a_blank_panel() {
    let empty = ContainerRollup::empty_healthy(1_000);
    assert!(empty.containers.is_empty());
    assert_eq!(page_branch(Some(&empty)), ContainersPageBranch::Empty);

    let app = crate::IcedApp::demo();
    let _view = render(&app);
}

#[test]
fn container_table_caps_materialized_rows_and_reports_the_overflow() {
    let (shown, hidden) = container_row_window(203);
    assert_eq!(shown, taskmanager_application::MAX_CONTAINER_ROWS);
    assert_eq!(hidden, 3);
    let label = more_rows_label(hidden);
    assert!(label.contains('3'));
    assert!(!label.contains("{count}"));
}
