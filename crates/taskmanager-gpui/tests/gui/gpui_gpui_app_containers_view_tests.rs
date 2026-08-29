//! Containers page tests: the pure row-VM folds are table-driven input→output
//! assertions; the page-branch and list-window contracts are render-path
//! window tests that prove WHICH branch painted and HOW MANY bounded rows
//! materialized — never a discarded render result.

use super::*;
use crate::gpui_app::root::{RootView, TopPage};
use gpui::{AppContext, TestAppContext, VisualTestContext, px};
use taskmanager_application::MAX_CONTAINER_ROWS;
use taskmanager_core::core::{DeviceState, FailureKind, ScalarObservation};

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

fn state_for(status: DeviceStatus) -> DeviceState {
    DeviceState::default().transition(status, 1_000)
}

fn wrapped_root(cx: &mut TestAppContext) -> (gpui::WindowHandle<RootView>, gpui::Entity<RootView>) {
    let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    let view = win.entity(cx).expect("window root RootView entity");
    (win, view)
}

fn draw(cx: &mut TestAppContext, win: gpui::WindowHandle<RootView>) {
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
}

/// Drive the Containers page with one rollup and return the drawn frame's
/// probe context.
fn containers_page(cx: &mut TestAppContext, rollup: ContainerRollup) -> VisualTestContext {
    let (win, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Containers;
        v.replace_containers_for_test(rollup);
        cx.notify();
    });
    draw(cx, win);
    VisualTestContext::from_window(win.into(), cx)
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

/// A populated healthy rollup paints exactly one bounded row per container —
/// including a row whose CPU is a typed first-sample gap — and paints no
/// empty/typed-state panel beside them.
#[gpui::test]
async fn populated_rollup_paints_one_bounded_row_per_container(cx: &mut TestAppContext) {
    let rollup = ContainerRollup {
        state: DeviceState::healthy(1_000),
        containers: vec![
            sample_container(Some(200.0), Some(100 * 1024 * 1024)),
            sample_container(None, None),
        ],
    };
    let mut vcx = containers_page(cx, rollup);
    for index in 0..2 {
        let selector: &'static str =
            Box::leak(format!("tm-containers-row:{index}").into_boxed_str());
        let row = vcx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("container row {index} must paint"));
        assert!(
            row.size.height > px(10.0) && row.size.width > px(100.0),
            "container row {index} collapsed: {row:?}"
        );
    }
    assert!(
        vcx.debug_bounds("tm-containers-row:2").is_none(),
        "no row may paint beyond the rollup's container count"
    );
    assert!(
        vcx.debug_bounds("tm-containers-empty").is_none()
            && vcx
                .debug_bounds("tm-containers-state-unsupported")
                .is_none(),
        "a populated rollup must not paint any empty/typed-state panel"
    );
    drop(vcx);
}

/// A container-free healthy host paints the explicit empty state — a real,
/// sized panel — and never a typed-failure panel or any data row.
#[gpui::test]
async fn empty_healthy_rollup_paints_the_explicit_empty_state(cx: &mut TestAppContext) {
    let rollup = ContainerRollup::empty_healthy(1_000);
    let mut vcx = containers_page(cx, rollup);
    let empty = vcx
        .debug_bounds("tm-containers-empty")
        .expect("the explicit empty state must paint, never a blank panel");
    assert!(
        empty.size.width > px(100.0) && empty.size.height > px(20.0),
        "the empty state must be a readable panel, not a collapsed stub: {empty:?}"
    );
    assert!(
        vcx.debug_bounds("tm-containers-state-unsupported")
            .is_none(),
        "a container-free healthy host must not masquerade as a failed source"
    );
    assert!(
        vcx.debug_bounds("tm-containers-row:0").is_none(),
        "no data row may paint for an empty rollup"
    );
    drop(vcx);
}

/// An unsupported source paints the typed reason — not the healthy empty
/// copy — so a cgroup-v1 host never reads as a container-free system.
#[gpui::test]
async fn unsupported_rollup_paints_the_typed_reason_not_the_empty_copy(cx: &mut TestAppContext) {
    let rollup = ContainerRollup::unavailable(state_for(DeviceStatus::Unsupported));
    let mut vcx = containers_page(cx, rollup);
    let typed = vcx
        .debug_bounds("tm-containers-state-unsupported")
        .expect("the typed unsupported panel must paint");
    assert!(
        typed.size.width > px(100.0) && typed.size.height > px(20.0),
        "the typed state must be a readable panel: {typed:?}"
    );
    assert!(
        vcx.debug_bounds("tm-containers-empty").is_none(),
        "an unsupported source must never share the healthy empty copy"
    );
    drop(vcx);
}

/// The list materializes at most the shared row window and reports the
/// remainder through the "+N more" hint instead of silently dropping rows.
#[gpui::test]
async fn oversized_rollup_caps_materialized_rows_and_reports_the_overflow(cx: &mut TestAppContext) {
    let containers: Vec<ContainerSummary> = (0..(MAX_CONTAINER_ROWS + 3))
        .map(|index| {
            let mut container = sample_container(Some((index % 100) as f32), None);
            container.id = format!("/docker/c{index}");
            container.name = format!("c{index}");
            container.cgroup_path = format!("/docker/c{index}");
            container
        })
        .collect();
    let rollup = ContainerRollup {
        state: DeviceState::healthy(1_000),
        containers,
    };
    let mut vcx = containers_page(cx, rollup);
    let last_shown: &'static str =
        Box::leak(format!("tm-containers-row:{}", MAX_CONTAINER_ROWS - 1).into_boxed_str());
    let first_beyond: &'static str =
        Box::leak(format!("tm-containers-row:{MAX_CONTAINER_ROWS}").into_boxed_str());
    assert!(
        vcx.debug_bounds(last_shown).is_some(),
        "the list must materialize through the shared row-window bound"
    );
    assert!(
        vcx.debug_bounds(first_beyond).is_none(),
        "no row may materialize beyond the shared row-window bound"
    );
    let more = vcx
        .debug_bounds("tm-containers-more")
        .expect("the overflow must be reported, never silently dropped");
    assert!(
        more.size.width > px(10.0),
        "the +N hint must paint as a real line: {more:?}"
    );
    drop(vcx);
}
