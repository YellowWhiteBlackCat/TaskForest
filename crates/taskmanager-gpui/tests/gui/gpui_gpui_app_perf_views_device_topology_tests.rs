//! Device-topology render regressions for the Disk page's partition panel
//! (`perf_views::partition_stats`, mounted by the shared device-page
//! composition root).
//!
//! `storage.device-topology` delivers the shared storage device/partition
//! topology; these tests prove the painted partition rows follow the typed
//! projection: a mounted partition paints its identity + usage row + a usage
//! bar whose fill is exactly the observed used/total fraction, an unmounted
//! partition stays in the topology as the compact summary line instead of a
//! fabricated full row, and a partition without observations paints no bar at
//! all rather than a 0% fill.

use gpui::{AppContext, TestAppContext, VisualTestContext, px, size};
use taskmanager_core::core::metrics::{
    DiskMetrics, DiskPartition, DiskPartitionScalarObservations, ScalarObservation,
};
use taskmanager_core::core::{DeviceGeneration, DeviceState};
use taskmanager_test_support::DiskMetricsFixtureBuilder;
use taskmanager_test_support::DiskPartitionFixtureBuilder;

use crate::gpui_app::root::{RootView, TopPage};
use crate::gpui_app::sidebar::SelectedDevice;
use taskmanager_theme::Theme;

const GIB: u64 = 1024 * 1024 * 1024;

fn gib(n: u64) -> u64 {
    n * GIB
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

/// `debug_bounds` takes a `&'static str` selector; the partition slots are
/// indexed, so the query strings are interned here.
fn selector(text: String) -> &'static str {
    Box::leak(text.into_boxed_str())
}

fn partition(
    name: &str,
    mount_point: &str,
    used: Option<u64>,
    capacity: Option<u64>,
) -> DiskPartition {
    let scalars = match (used, capacity) {
        (Some(used), Some(capacity)) => DiskPartitionScalarObservations {
            capacity_bytes: ScalarObservation::available(capacity, 10),
            used_bytes: ScalarObservation::available(used, 10),
            free_bytes: ScalarObservation::available(capacity.saturating_sub(used), 10),
        },
        _ => DiskPartitionScalarObservations::default(),
    };
    DiskPartitionFixtureBuilder::new()
        .device_id(format!("partition:fixture:{name}"))
        .parent_device_id("fixture".into())
        .device_generation(DeviceGeneration::new(1))
        .device_state(DeviceState::healthy(10))
        .name(name.into())
        .mount_point(mount_point.into())
        .fs_type("ext4".into())
        .scalar_observations(scalars)
        .build()
}

fn disk_with(partitions: Vec<DiskPartition>) -> DiskMetrics {
    DiskMetricsFixtureBuilder::new()
        .device_id("fixture".into())
        .name("nvme0n1".into())
        .disk_type("NVMe SSD".into())
        .current_capacity_bytes(gib(2000))
        .current_available_bytes(gib(1200))
        .partitions(partitions)
        .build()
}

/// The painted topology, end to end: every mounted partition paints its own
/// identity/usage/bar row, the bar fill is the observed used/total fraction
/// (40% and 80% here), and the unmounted partition stays addressable through
/// the compact summary line instead of a fabricated full row.
#[gpui::test]
async fn disk_page_paints_each_partition_row_with_its_observed_usage_fraction(
    cx: &mut TestAppContext,
) {
    let (win, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Performance;
        v.selected = SelectedDevice::Disk(0);
        v.system_snapshot_mut_for_test().disks = vec![disk_with(vec![
            partition("nvme0n1p1", "/", Some(gib(200)), Some(gib(500))),
            partition("nvme0n1p2", "/home", Some(gib(800)), Some(gib(1000))),
            partition("nvme0n1p3", "", None, None),
        ])];
        cx.notify();
    });
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);

    for index in 0..2 {
        for slot in ["", "-label", "-usage", "-bar", "-fill"] {
            let sel = selector(format!("tm-disk-partition{slot}:{index}"));
            let bounds = vcx
                .debug_bounds(sel)
                .unwrap_or_else(|| panic!("mounted partition {index} must paint its {slot} slot"));
            assert!(
                bounds.size.width > px(0.0) && bounds.size.height > px(0.0),
                "partition slot {sel} collapsed: {bounds:?}"
            );
        }
    }
    // An unmounted partition has no trusted usage: the full row must not be
    // fabricated, while the unit stays in the topology summary.
    assert!(
        vcx.debug_bounds("tm-disk-partition:2").is_none(),
        "an unmounted partition must not paint a full usage row"
    );
    assert!(
        vcx.debug_bounds("tm-disk-partition-fill:2").is_none(),
        "an unmounted partition must not paint a usage bar"
    );
    let summary = vcx
        .debug_bounds("tm-disk-partitions-unmounted")
        .expect("an unmounted partition must stay visible as the summary line");
    assert!(summary.size.width > px(0.0));

    let mut row_tops = Vec::new();
    for (index, expected) in [(0_usize, 0.40_f32), (1, 0.80)] {
        let row = vcx
            .debug_bounds(selector(format!("tm-disk-partition:{index}")))
            .expect("mounted partition row");
        let bar = vcx
            .debug_bounds(selector(format!("tm-disk-partition-bar:{index}")))
            .expect("mounted partition bar");
        let fill = vcx
            .debug_bounds(selector(format!("tm-disk-partition-fill:{index}")))
            .expect("mounted partition fill");
        let ratio = f32::from(fill.size.width) / f32::from(bar.size.width);
        assert!(
            (ratio - expected).abs() <= 0.02,
            "partition {index} fill must match the observed used/total fraction {expected}: ratio={ratio}, bar={bar:?}, fill={fill:?}"
        );
        row_tops.push(f32::from(row.origin.y));
    }
    assert!(
        row_tops[1] > row_tops[0],
        "partition rows must stack in projected order: {row_tops:?}"
    );
}

/// Honest absence: a mounted partition whose usage was never observed keeps
/// its identity row and the typed unavailable text, but paints no bar (and no
/// fill selector) — never a fabricated 0% usage.
#[gpui::test]
async fn mounted_partition_without_measurements_paints_no_fabricated_usage_bar(
    cx: &mut TestAppContext,
) {
    let (win, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Performance;
        v.selected = SelectedDevice::Disk(0);
        v.system_snapshot_mut_for_test().disks =
            vec![disk_with(vec![partition("nvme0n1p1", "/", None, None)])];
        cx.notify();
    });
    cx.simulate_window_resize(win.into(), size(px(1180.0), px(780.0)));
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    assert!(
        vcx.debug_bounds("tm-disk-partition:0").is_some()
            && vcx.debug_bounds("tm-disk-partition-usage:0").is_some(),
        "an unobserved mounted partition keeps its addressed usage slot"
    );
    assert!(
        vcx.debug_bounds("tm-disk-partition-fill:0").is_none(),
        "an unobserved partition must not paint a 0% usage fill"
    );
    assert!(
        vcx.debug_bounds("tm-disk-partitions-unmounted").is_none(),
        "a mounted partition is never folded into the unmounted summary"
    );
}
