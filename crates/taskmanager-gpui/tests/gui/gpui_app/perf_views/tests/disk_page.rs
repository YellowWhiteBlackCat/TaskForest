//! Disk performance page render and geometry behavior.

use super::*;

/// Render-path assertion (后置): a device page built by the shared
/// `perf_page` composition root (disk / network / GPU) must paint its data-driven
/// title and its stats column when a device snapshot row exists.
#[gpui::test]
async fn mc02_partition_case_disk_page_paints_title_and_stats_from_device_data(
    cx: &mut TestAppContext,
) {
    let (win, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Performance;
        v.selected = SelectedDevice::Disk(0);
        v.system_snapshot_mut_for_test().disks = vec![
            DiskMetricsFixtureBuilder::new()
                .device_id("nvme0n1".into())
                .name("nvme0n1".into())
                .disk_type("NVMe SSD".into())
                .model("Test NVMe 2TB".into())
                .fs_type("ext4".into())
                .mount_point("/".into())
                .serial(Some(
                    "A-DELIBERATELY-LONG-SERIAL-NUMBER-FOR-LAYOUT-REGRESSION-0420".into(),
                ))
                .current_capacity_bytes(gib(2000))
                .current_available_bytes(gib(1200))
                .partitions(vec![
                    DiskPartitionFixtureBuilder::new()
                        .device_id("partition:nvme0n1:nvme0n1p1".into())
                        .parent_device_id("nvme0n1".into())
                        .device_generation(DeviceGeneration::new(1))
                        .device_state(DeviceState::healthy(10))
                        .name("nvme0n1p1".into())
                        .mount_point(
                            "/mnt/this-is-a-deliberately-long-mount-point-for-layout-regression"
                                .into(),
                        )
                        .fs_type("ext4".into())
                        .scalar_observations(DiskPartitionScalarObservations {
                            capacity_bytes: ScalarObservation::available(gib(500), 10),
                            used_bytes: ScalarObservation::available(gib(200), 10),
                            free_bytes: ScalarObservation::available(gib(300), 10),
                        })
                        .build(),
                    DiskPartitionFixtureBuilder::new()
                        .device_id("partition:nvme0n1:nvme0n1p2".into())
                        .parent_device_id("nvme0n1".into())
                        .device_generation(DeviceGeneration::new(1))
                        .device_state(DeviceState::healthy(10))
                        .name("nvme0n1p2".into())
                        .build(),
                ])
                .build(),
        ];
        cx.notify();
    });
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let title = vcx
        .debug_bounds("tm-perf-title")
        .expect("disk page must paint its title row");
    assert!(title.size.height > px(20.0), "title collapsed: {title:?}");
    assert!(title.size.width > px(0.0), "title has no width: {title:?}");
    let panel = vcx
        .debug_bounds("tm-perf-stats-panel")
        .expect("disk page must paint its stats column");
    let title_text = vcx
        .debug_bounds("tm-perf-title-text")
        .expect("disk page must expose a bounded title text slot");
    let subtitle_text = vcx
        .debug_bounds("tm-perf-subtitle-text")
        .expect("disk page must expose a bounded subtitle text slot");
    assert!(
        panel.size.width > px(200.0) && panel.size.height > px(100.0),
        "stats panel geometry collapsed: {panel:?}"
    );
    assert!(
        title_text.origin.x + title_text.size.width <= subtitle_text.origin.x + px(0.5)
            || subtitle_text.origin.y >= title_text.origin.y + title_text.size.height - px(0.5),
        "long device identity and context must stay in their own slots (truncate or wrap), never overlap: title={title_text:?}, subtitle={subtitle_text:?}"
    );
    // Every stats value — including the long serial readout — must stay on
    // its bounded row inside the pinned stats surface instead of clipping
    // past the panel's right edge.
    for i in 0..16 {
        let selector: &'static str = Box::leak(format!("tm-perf-stat-value:{i}").into_boxed_str());
        let Some(value) = vcx.debug_bounds(selector) else {
            break;
        };
        assert!(
            value.origin.x >= panel.origin.x - px(0.5)
                && value.origin.x + value.size.width <= panel.origin.x + panel.size.width + px(0.5),
            "stats value {i} must render inside the stats panel: value={value:?}, panel={panel:?}"
        );
    }
    assert!(
        title_text.origin.x + title_text.size.width <= panel.origin.x,
        "compact title text must not intrude into the stats panel: title={title_text:?}, panel={panel:?}"
    );
    assert!(
        subtitle_text.origin.x + subtitle_text.size.width <= panel.origin.x,
        "compact subtitle text must not intrude into the stats panel: subtitle={subtitle_text:?}, panel={panel:?}"
    );
    let r0 = vcx
        .debug_bounds("tm-perf-stat:0")
        .expect("first stats row must render");
    assert!(r0.size.height > px(10.0), "stats row collapsed: {r0:?}");
    let graph = vcx
        .debug_bounds("tm-perf-chart-card:main-graph")
        .expect("disk page must paint the shared main graph card");
    assert!(
        graph.size.height >= px(180.0),
        "shared disk graph must retain the headline tier floor: {graph:?}"
    );
    let partition = vcx
        .debug_bounds("tm-disk-partition:0")
        .expect("disk page must paint the mounted partition row");
    let partition_label = vcx
        .debug_bounds("tm-disk-partition-label:0")
        .expect("partition identity must have its own bounded row");
    let partition_usage = vcx
        .debug_bounds("tm-disk-partition-usage:0")
        .expect("partition usage must have its own bounded row");
    let partition_bar = vcx
        .debug_bounds("tm-disk-partition-bar:0")
        .expect("partition progress bar must render independently");
    assert!(
        partition_label.size.width > px(0.0)
            && partition_usage.size.width > px(0.0)
            && partition_bar.size.width >= partition.size.width - px(1.0)
            && partition_bar.size.height >= px(6.0)
            && partition_label.origin.x + partition_label.size.width
                <= partition.origin.x + partition.size.width + px(0.5)
            && partition_usage.origin.x + partition_usage.size.width
                <= partition.origin.x + partition.size.width + px(0.5),
        "long mount-point identity and usage rows must remain independently bounded: partition={partition:?}, label={partition_label:?}, usage={partition_usage:?}"
    );
    let partitions = vcx
        .debug_bounds("tm-disk-partitions")
        .expect("disk page must paint the partition panel");
    assert!(
        partitions.size.width > px(200.0) && partitions.size.height > px(40.0),
        "partition panel geometry collapsed: {partitions:?}"
    );
    assert!(
        vcx.debug_bounds("tm-disk-partition:0").is_some(),
        "mounted partition row must render"
    );
    assert!(
        vcx.debug_bounds("tm-disk-partitions-unmounted").is_some(),
        "unmounted partitions must render as one compact summary line"
    );
    drop(vcx);

    // The shared page viewport must constrain the fixed stats column at every
    // supported aspect ratio. This catches a missing min-width boundary in the
    // shell, which otherwise lets the graph/title intrinsic width push the
    // right-hand panel beyond the actual window. The width-aware budget pins
    // the rail at every supported size (the workspace always reserves the
    // stats minimum), so containment holds across the whole range.
    for (width, height) in [
        (720.0f32, 480.0f32),
        (720.0, 680.0),
        (1180.0, 780.0),
        (1900.0, 1344.0),
    ] {
        cx.simulate_window_resize(win.into(), size(px(width), px(height)));
        draw(cx, win);
        let mut vcx = VisualTestContext::from_window(win.into(), cx);
        let panel = vcx
            .debug_bounds("tm-perf-stats-panel")
            .expect("disk stats panel must remain rendered after resize");
        let graph = vcx
            .debug_bounds("tm-perf-chart-card:main-graph")
            .expect("disk graph must remain rendered after resize");
        assert!(
            f32::from(panel.origin.x) >= -0.5
                && f32::from(panel.origin.x + panel.size.width) <= width + 0.5
                && f32::from(panel.origin.y) >= -0.5
                && f32::from(panel.origin.y + panel.size.height) <= height + 0.5,
            "disk stats panel must stay inside the {width}x{height} viewport: {panel:?}"
        );
        assert!(
            graph.size.height >= px(180.0),
            "disk graph must hold the headline tier floor at {width}x{height}: {graph:?}"
        );
        if width == 720.0 && height == 480.0 {
            assert!(
                vcx.debug_bounds("tm-disk-partitions").is_none(),
                "the short viewport must yield the complete lower band before it clips the headline"
            );
        } else if width == 720.0 {
            let partitions = vcx.debug_bounds("tm-disk-partitions");
            assert!(
                partitions.is_some(),
                "compact disk page must keep the partition panel addressable: viewport={:?}, graph={:?}, vital={:?}, title={:?}",
                vcx.debug_bounds("tm-perf-main-viewport"),
                vcx.debug_bounds("tm-perf-chart-card:main-graph"),
                vcx.debug_bounds("tm-perf-vital-line"),
                vcx.debug_bounds("tm-perf-title"),
            );
            let partitions = partitions.expect("asserted above");
            assert!(
                graph.bottom() <= partitions.origin.y + px(0.5),
                "compact disk partition panel must follow, never cover, the headline graph: graph={graph:?}, partitions={partitions:?}"
            );
            if let Some(usage) = vcx.debug_bounds("tm-disk-usage-panel") {
                assert!(
                    partitions.bottom() <= usage.origin.y + px(0.5),
                    "compact disk usage panel must follow the partition panel: partitions={partitions:?}, usage={usage:?}"
                );
            }
        }
        drop(vcx);
    }
}
