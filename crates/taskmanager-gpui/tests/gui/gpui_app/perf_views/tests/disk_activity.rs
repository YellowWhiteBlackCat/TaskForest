//! Active-time (disk busy %) render regressions for the Disk page's
//! secondary graph, split from the shared perf-views suite to keep every
//! module under the line guard.

use super::*;

use std::collections::BTreeMap;
use taskmanager_core::core::{
    DeviceLifecycle, DevicePresence, DiskScalarObservations, StorageTelemetryObservation,
};

/// The Disk page's active-time card consumes this disk's OWN generation-scoped
/// activity ring: with accepted observations the page paints the secondary
/// percentage graph, and the window the card plots is exactly the store's
/// per-device activity evidence.
#[gpui::test]
async fn disk_page_projects_the_active_time_graph_from_its_own_ring(cx: &mut TestAppContext) {
    let activity_disk = |active_pct: f32| {
        taskmanager_test_support::DiskMetricsFixtureBuilder::new()
            .device_id("disk:wwid:activity".into())
            .device_generation(DeviceGeneration::new(1))
            .device_state(DeviceState::healthy(10))
            .scalar_observations(DiskScalarObservations {
                active_time_pct: ScalarObservation::available(active_pct, 10),
                ..Default::default()
            })
            .build()
    };
    let lifecycle = DeviceLifecycle {
        presence: DevicePresence::Present,
        state: DeviceState::healthy(10),
        generation: DeviceGeneration::INITIAL,
        first_seen_ms: Some(10),
        last_seen_ms: Some(10),
        absent_since_ms: None,
    };

    let (win, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Performance;
        for (revision, active_pct) in [(1_u64, 40.0_f32), (2, 80.0)] {
            let observation = StorageTelemetryObservation::current(
                vec![activity_disk(active_pct)],
                10,
                Vec::new(),
                Vec::new(),
                BTreeMap::from([(DeviceId::new("disk:wwid:activity"), lifecycle)]),
            );
            v.telemetry_ingestor
                .ingest_correlated_storage(
                    CorrelatedTelemetryStamp::from_accepted_event(revision, 10)
                        .expect("fixture revision is non-zero"),
                    &observation,
                )
                .expect("activity fixture enters system history");
        }
        v.system_snapshot_mut_for_test().disks = vec![
            taskmanager_test_support::DiskMetricsFixtureBuilder::new()
                .device_id("disk:wwid:activity".into())
                .device_generation(DeviceGeneration::new(1))
                .device_state(DeviceState::healthy(10))
                .name("activity0n1".into())
                .build(),
        ];
        v.selected = SelectedDevice::Disk(0);
        cx.notify();
    });
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    assert!(
        vcx.debug_bounds("tm-perf-secondary-graph:disk-activity-graph")
            .is_some(),
        "recorded activity must render the disk page's secondary graph"
    );
    let activity_graph = vcx
        .debug_bounds("tm-graph:disk-activity-graph")
        .expect("the activity graph must paint its chart canvas");
    assert!(
        activity_graph.size.height >= px(120.0),
        "activity graph must retain a readable height: {activity_graph:?}"
    );
    drop(vcx);

    // The window the card plots is this disk's own activity ring, never a
    // sibling's or the host-wide mean.
    view.read_with(cx, |v, _| {
        use crate::gpui_app::history_samples::storage_activity_samples;
        assert_eq!(
            &*storage_activity_samples(
                &v.telemetry.system_history,
                "disk:wwid:activity",
                DeviceGeneration::new(1),
            ),
            &[40.0, 80.0][..]
        );
    });
}

/// A disk whose activity ring holds no sample renders no secondary graph —
/// the honest absence, never a fabricated flat 0% curve. Proven on a fresh
/// window so the assertion cannot ride a stale rendered frame.
#[gpui::test]
async fn disk_without_activity_samples_renders_no_activity_graph(cx: &mut TestAppContext) {
    let (win, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Performance;
        v.system_snapshot_mut_for_test().disks = vec![
            taskmanager_test_support::DiskMetricsFixtureBuilder::new()
                .device_id("disk:wwid:cold".into())
                .device_generation(DeviceGeneration::new(1))
                .device_state(DeviceState::healthy(10))
                .name("cold0n1".into())
                .build(),
        ];
        v.selected = SelectedDevice::Disk(0);
        cx.notify();
    });
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    assert!(
        vcx.debug_bounds("tm-perf-secondary-graph:disk-activity-graph")
            .is_none(),
        "a disk without activity samples must not fabricate an activity curve"
    );
}
