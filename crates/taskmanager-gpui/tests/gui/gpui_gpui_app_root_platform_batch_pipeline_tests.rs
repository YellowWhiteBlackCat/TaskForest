use gpui::AppContext;
use taskmanager_application::{
    ContainerRollupEvent, CorrelatedEvent, PlatformEventBatch, PlatformEventContext, ProcessEvent,
};
use taskmanager_core::core::identity::ProviderId;
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process::ProcessItem;
use taskmanager_core::core::{ContainerRollup, ContainerSummary, DeviceState, IsolationKind};
use taskmanager_platform_contract::{CapabilityId, EventSequence, RequestId};

use super::RootView;
use taskmanager_theme::Theme;

fn process(pid: u32, name: &str, cpu: f32, mem: u64) -> ProcessItem {
    taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(pid)
        .parent_pid(Some(1))
        .name(name.into())
        .cmdline(format!("{name} --flag"))
        .scalar_observations(taskmanager_core::core::process::ProcessScalarObservations {
            start_token: ScalarObservation::available(u64::from(pid) + 10_000, 1_000),
            ..Default::default()
        })
        .current_cpu_percentage(cpu)
        .current_memory_bytes(mem)
        .current_disk_read_bytes_per_sec(0)
        .current_disk_write_bytes_per_sec(0)
        .status("S".into())
        .metadata_observations(
            taskmanager_core::core::process::ProcessMetadataObservations::current(
                taskmanager_core::core::process::ProcessOwner::opaque("root"),
                None,
                1,
            ),
        )
        .build()
}

fn snapshot_batch(processes: Vec<ProcessItem>) -> PlatformEventBatch {
    PlatformEventBatch {
        process_events: vec![CorrelatedEvent::new(
            PlatformEventContext {
                request_id: RequestId::new(7).expect("request id"),
                capability: CapabilityId::PROCESS_LIST,
                provider: Some(ProviderId::borrowed("linux.procfs")),
                sequence: EventSequence::new(1),
                observed_at_ms: 1000,
            },
            ProcessEvent::Snapshot(std::sync::Arc::new(processes)),
        )],
        ..Default::default()
    }
}

/// Data-pipeline regression (前置): a process snapshot crossing the batch
/// boundary must reach `RootView::processes` with its typed values intact —
/// "did the data arrive, and is the value right?"
#[gpui::test]
async fn process_snapshot_reaches_root_view_unchanged(cx: &mut gpui::TestAppContext) {
    let win = cx.add_window(|_w, cx| RootView::new(Theme::dark(), cx));
    let batch = snapshot_batch(vec![
        process(4242, "important-worker", 12.5, 1_048_576),
        process(4243, "second", 0.0, 2048),
    ]);
    cx.update_window(win.into(), |view, _window, cx| {
        let entity = view
            .downcast::<RootView>()
            .expect("window root is RootView");
        entity.update(cx, |v, cx| {
            let changes = v.apply_platform_event_batch(batch, cx);
            assert!(changes.processes, "process snapshot must flag the change");
            assert_eq!(v.processes().len(), 2, "both processes must arrive");
            assert_eq!(v.processes()[0].pid, 4242);
            assert_eq!(v.processes()[0].name, "important-worker");
            assert_eq!(
                v.processes()[0].current_cpu_percentage(),
                Some(12.5),
                "CPU value must survive intact"
            );
            assert_eq!(
                v.processes()[0].current_memory_bytes(),
                Some(1_048_576),
                "memory value must survive intact"
            );
            assert_eq!(v.processes()[1].pid, 4243);
        });
    })
    .unwrap();
}

/// An empty snapshot is a legitimately observed empty list (fail-closed,
/// not stale): the flag still fires and the list becomes empty.
#[gpui::test]
async fn empty_snapshot_clears_the_list_and_flags_change(cx: &mut gpui::TestAppContext) {
    let win = cx.add_window(|_w, cx| RootView::new(Theme::dark(), cx));
    cx.update_window(win.into(), |view, _window, cx| {
        let entity = view
            .downcast::<RootView>()
            .expect("window root is RootView");
        entity.update(cx, |v, cx| {
            let first =
                v.apply_platform_event_batch(snapshot_batch(vec![process(1, "a", 1.0, 1)]), cx);
            assert!(first.processes && v.processes().len() == 1);
            let second = v.apply_platform_event_batch(snapshot_batch(vec![]), cx);
            assert!(second.processes, "empty snapshot is still a change");
            assert!(v.processes().is_empty());
        });
    })
    .unwrap();
}

/// One accepted raw batch advances the shared process revision exactly once.
/// Render memoization and input targeting then consume that same materialized
/// generation; an unrelated domain update neither replaces the process `Rc`
/// nor rebuilds the render projection.
#[gpui::test]
async fn process_materialization_is_single_fold_and_shared_by_render_and_input(
    cx: &mut gpui::TestAppContext,
) {
    let win = cx.add_window(|_w, cx| RootView::new(Theme::dark(), cx));
    cx.update_window(win.into(), |view, _window, cx| {
        let entity = view
            .downcast::<RootView>()
            .expect("window root is RootView");
        entity.update(cx, |view, cx| {
            let changes = view.apply_platform_event_batch(
                snapshot_batch(vec![process(4242, "generation-worker", 7.0, 4096)]),
                cx,
            );
            assert!(changes.processes);
            let generation = view.processes_generation();
            assert_eq!(generation, 1, "one raw batch must fold exactly once");
            assert_eq!(generation, view.projection().process_revision);

            let process_snapshot = view.processes_arc().clone();
            let (render_rows, _, _) = view.processes_projection();
            let rendered_identity = render_rows
                .iter()
                .find_map(|row| row.process_identity)
                .expect("canonical category tree includes the process row");
            assert_eq!(rendered_identity.pid(), 4242);

            view.select_process_single(rendered_identity);
            view.request_end_task_confirmation(rendered_identity);
            let confirmation = view
                .pending_confirmation()
                .expect("input path freezes the visible process");
            let taskmanager_application::PendingConfirmation::EndTask(target) = confirmation else {
                panic!("single-process end must use the shared EndTask branch")
            };
            assert_eq!(target.pid, rendered_identity.pid());
            assert_eq!(view.processes_generation(), generation);

            let unrelated =
                view.apply_platform_event_batch(container_rollup_batch(populated_rollup()), cx);
            assert!(!unrelated.processes);
            let (reused_rows, _, _) = view.processes_projection();
            assert_eq!(view.processes_generation(), generation);
            assert!(std::sync::Arc::ptr_eq(
                &process_snapshot,
                view.processes_arc(),
            ));
            assert!(std::rc::Rc::ptr_eq(&render_rows, &reused_rows));
        });
    })
    .unwrap();
}

/// Build a PlatformEventBatch carrying one correlated container rollup
/// snapshot. This is the exact shape the runtime publishes when the
/// ContainerRollupCollector drains on its observation lane.
fn container_rollup_batch(rollup: ContainerRollup) -> PlatformEventBatch {
    PlatformEventBatch {
        containers_events: vec![CorrelatedEvent::new(
            PlatformEventContext {
                request_id: RequestId::new(11).expect("request id"),
                capability: CapabilityId::CONTAINERS,
                provider: Some(ProviderId::borrowed("linux.containers.cgroup-v2")),
                sequence: EventSequence::new(1),
                observed_at_ms: 1_000,
            },
            ContainerRollupEvent::Snapshot(Box::new(rollup)),
        )],
        ..Default::default()
    }
}

fn populated_rollup() -> ContainerRollup {
    ContainerRollup {
        state: DeviceState::healthy(1_000),
        containers: vec![ContainerSummary {
            id: "/docker/abc123".into(),
            name: "abc123".into(),
            runtime: Some(IsolationKind::Docker),
            cgroup_path: "/docker/abc123".into(),
            cpu_percentage: ScalarObservation::available(42.0, 1_000),
            memory_bytes: ScalarObservation::available(256 * 1024 * 1024, 1_000),
            member_pids: vec![100, 101],
        }],
    }
}

/// End-to-end wire proof: a ContainerRollupEvent::Snapshot crossing the
/// batch boundary must reach `RootView.containers` with its typed values
/// intact — NOT the default `empty_healthy(0)`. This is the regression
/// that catches the honesty bug where the page always showed "no
/// containers" even when containers exist.
#[gpui::test]
async fn container_rollup_snapshot_reaches_root_view_unchanged(cx: &mut gpui::TestAppContext) {
    let win = cx.add_window(|_w, cx| RootView::new(Theme::dark(), cx));
    let rollup = populated_rollup();
    cx.update_window(win.into(), |view, _window, cx| {
        let entity = view
            .downcast::<RootView>()
            .expect("window root is RootView");
        entity.update(cx, |v, cx| {
            // Sanity: the view starts with the default empty-healthy rollup.
            assert!(
                v.containers().containers.is_empty(),
                "view must start with the default empty rollup"
            );
            let _changes = v.apply_platform_event_batch(container_rollup_batch(rollup.clone()), cx);
            assert_eq!(
                v.containers(),
                &rollup,
                "container rollup must arrive intact after batch apply"
            );
            assert_eq!(v.containers().containers.len(), 1);
            assert_eq!(v.containers().containers[0].id, "/docker/abc123");
            assert_eq!(
                v.containers().containers[0].cpu_percentage.current_value(),
                Some(&42.0),
                "CPU% must survive the wire intact"
            );
        });
    })
    .unwrap();
}

/// A healthy empty rollup (no containers running) must also propagate —
/// it is a real, honest state, distinct from the default `empty_healthy(0)`.
#[gpui::test]
async fn empty_healthy_rollup_propagates_and_is_not_the_default(cx: &mut gpui::TestAppContext) {
    let win = cx.add_window(|_w, cx| RootView::new(Theme::dark(), cx));
    cx.update_window(win.into(), |view, _window, cx| {
        let entity = view
            .downcast::<RootView>()
            .expect("window root is RootView");
        entity.update(cx, |v, cx| {
            let observed_at = 5_000_u64;
            let rollup = ContainerRollup::empty_healthy(observed_at);
            let _changes = v.apply_platform_event_batch(container_rollup_batch(rollup.clone()), cx);
            // The rollup arrived (not the default empty_healthy(0)):
            assert_eq!(v.containers().state.last_success_ms, Some(observed_at));
            assert!(v.containers().containers.is_empty());
        });
    })
    .unwrap();
}
