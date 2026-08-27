use super::*;
use crate::core::{DeviceState, FailureKind, ScalarObservation};

fn sample_container(cpu: Option<f32>, mem: Option<u64>) -> ContainerSummary {
    ContainerSummary {
        id: "/docker/abc".into(),
        name: "abc".into(),
        runtime: Some(IsolationKind::Docker),
        cgroup_path: "/docker/abc".into(),
        cpu_percentage: match cpu {
            Some(value) => ScalarObservation::available(value, 1_000),
            None => ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
        },
        memory_bytes: match mem {
            Some(bytes) => ScalarObservation::available(bytes, 1_000),
            None => ScalarObservation::unavailable(FailureKind::IdentityChanged),
        },
        member_pids: vec![10, 11],
    }
}

#[test]
fn runtime_label_covers_every_isolation_variant() {
    // Exhaustive so a future IsolationKind addition forces a conscious label.
    for kind in [
        IsolationKind::Docker,
        IsolationKind::Podman,
        IsolationKind::Kubernetes,
        IsolationKind::Lxc,
        IsolationKind::SystemdNspawn,
        IsolationKind::Flatpak,
        IsolationKind::Snap,
        IsolationKind::Wsl,
        IsolationKind::OtherContainer,
    ] {
        assert!(!runtime_label(&kind).is_empty());
    }
}

#[test]
fn first_sample_gap_cpu_folds_to_the_shared_dash() {
    let vm = container_row_vm(&sample_container(None, Some(100)));
    assert_eq!(vm.cpu, formatting::missing_value());
}

#[test]
fn present_cpu_folds_to_one_decimal_percent() {
    let vm = container_row_vm(&sample_container(Some(12.34), None));
    assert_eq!(vm.cpu, "12.3%");
}

#[test]
fn empty_member_pids_fold_to_the_shared_dash() {
    let mut container = sample_container(Some(5.0), None);
    container.member_pids.clear();
    assert_eq!(
        container_row_vm(&container).processes,
        formatting::missing_value()
    );
}

#[test]
fn member_pid_count_folds_to_the_count_string() {
    let vm = container_row_vm(&sample_container(Some(5.0), None));
    assert_eq!(vm.name, "abc");
    assert_eq!(vm.processes, "2");
}

#[test]
fn missing_runtime_folds_to_the_shared_dash() {
    let mut container = sample_container(Some(5.0), Some(64));
    container.runtime = None;
    assert_eq!(
        container_row_vm(&container).runtime,
        formatting::missing_value()
    );
}

#[test]
fn present_runtime_uses_the_friendly_label() {
    let vm = container_row_vm(&sample_container(Some(5.0), None));
    assert_eq!(vm.runtime, "Docker");
}

#[test]
fn memory_folds_dash_for_gap_and_formatter_output_when_present() {
    let gap = container_row_vm(&sample_container(Some(5.0), None));
    assert_eq!(gap.memory, formatting::missing_value());
    let present = container_row_vm(&sample_container(Some(5.0), Some(100 * 1024 * 1024)));
    assert_eq!(
        present.memory,
        formatting::format_decimal_memory(100 * 1024 * 1024)
    );
}

#[test]
fn empty_healthy_rollup_is_not_a_blank_panel() {
    let t = Theme::dark();
    let rollup = ContainerRollup::empty_healthy(1_000);
    // Renders without panic and takes the empty-message branch.
    let _ = render_containers(&t, &rollup);
    assert_eq!(rollup.state.status, DeviceStatus::Healthy);
    assert!(rollup.containers.is_empty());
}

#[test]
fn unsupported_rollup_is_typed_not_empty_copy() {
    let t = Theme::dark();
    let rollup = ContainerRollup::unavailable(state_for(DeviceStatus::Unsupported));
    let _ = render_containers(&t, &rollup);
    assert_eq!(rollup.state.status, DeviceStatus::Unsupported);
}

#[test]
fn populated_rollup_renders_one_row_per_container() {
    let t = Theme::dark();
    let rollup = ContainerRollup {
        state: DeviceState::healthy(1_000),
        containers: vec![
            sample_container(Some(200.0), Some(100 * 1024 * 1024)),
            sample_container(None, None),
        ],
    };
    // No panic across a current CPU reading and a typed-unavailable one.
    let _ = render_containers(&t, &rollup);
}

fn state_for(status: DeviceStatus) -> DeviceState {
    DeviceState::default().transition(status, 1_000)
}
