//! Headless render-geometry regression tests for the Performance page
//! (CPU per-core grid + the Memory/Disk/Network/GPU title + stats panels).
//!
//! Guard for "data arrived but the UI shows nothing": with typed snapshot data
//! present, the page must paint real layout/row elements with sane bounds.
//! Canvas graphs are deliberately not probed (debug_bounds cannot measure
//! them) — the assertions target rows, titles, and layout containers, and the
//! data-driven row/cell counts prove rendering follows the snapshot.

use gpui::{AppContext, TestAppContext, VisualTestContext, px, size};
use taskmanager_telemetry_store::CorrelatedTelemetryStamp;

use crate::core::metrics::{
    CpuMetrics, CpuScalarObservations, DiskPartitionScalarObservations, GpuEngine, GpuEngineKind,
    GpuMetrics, GpuScalarObservations, MemoryCompositionObservations,
    MemoryCompressionObservations, MemoryMetrics, MemoryOptionalObservations,
    MemoryScalarObservations, OptionalObservation, ScalarObservation, ScalarObservationGroup,
    VirtualMemoryCommitObservations,
};
use crate::core::{
    BatteryInfo, BatteryScalarObservations, DeviceGeneration, DeviceId, DeviceState,
    PowerSupplySnapshot, SensorCenterSnapshot, SensorDescriptor, SensorMagnitude,
    SensorMeasurementObservation, SensorReading, SensorScale,
};
use crate::gpui_app::root::{RootView, TopPage};
use crate::gpui_app::sidebar::SelectedDevice;
use crate::gpui_app::theme::Theme;

#[path = "tests/cpu.rs"]
mod cpu;
#[path = "tests/disk_activity.rs"]
mod disk_activity;
#[path = "tests/fixtures.rs"]
mod fixtures;
#[path = "tests/gpu_chart_metric.rs"]
mod gpu_chart_metric;
#[path = "tests/root_contract.rs"]
mod root_contract;
#[path = "tests/split_lanes.rs"]
mod split_lanes;
use fixtures::{sensor_reading, with_battery_scalars};

fn gib(n: u64) -> u64 {
    n * 1024 * 1024 * 1024
}

fn mib(n: u64) -> u64 {
    n * 1024 * 1024
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

/// The 8 always-present Memory stats rows (in_use, available, hardware_reserved,
/// cached, buffers, swap, speed, slots). Data-gated rows (committed / zram /
/// zswap / usage rate) append after these.
const MEM_BASE_STAT_ROWS: [&str; 8] = [
    "tm-perf-stat:0",
    "tm-perf-stat:1",
    "tm-perf-stat:2",
    "tm-perf-stat:3",
    "tm-perf-stat:4",
    "tm-perf-stat:5",
    "tm-perf-stat:6",
    "tm-perf-stat:7",
];

/// A first-frame Performance page must explain an empty history instead of
/// presenting a large, silent canvas. The state overlay remains during the
/// one-point warmup and is removed once two finite observations are available;
/// the graph never receives a fake zero just to make the overlay disappear.
#[gpui::test]
async fn performance_graph_explains_warmup_before_first_sample(cx: &mut TestAppContext) {
    let (win, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Performance;
        v.selected = SelectedDevice::Memory;
        cx.notify();
    });
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let state = vcx
        .debug_bounds("tm-graph-state")
        .expect("an empty first-frame graph must expose its collecting state");
    assert!(
        state.size.width > px(80.0) && state.size.height > px(20.0),
        "graph state badge must remain readable: {state:?}"
    );
}

/// Render-path assertion (后置): the Memory stats column must render one row
/// per derived stat, and the data-gated rows (committed / zram / zswap / usage
/// rate) must appear exactly when the snapshot carries the matching data.
#[gpui::test]
async fn memory_page_stats_rows_follow_snapshot_data(cx: &mut TestAppContext) {
    let (win, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Performance;
        v.selected = SelectedDevice::Memory;
        v.system_snapshot_mut_for_test().memory = MemoryMetrics::from_observations(
            MemoryScalarObservations {
                total_bytes: ScalarObservation::available(gib(16), 10),
                used_bytes: ScalarObservation::available(gib(6), 10),
                available_bytes: ScalarObservation::available(gib(10), 10),
                swap_total_bytes: ScalarObservation::available(gib(2), 10),
                swap_used_bytes: ScalarObservation::available(mib(512), 10),
                ..Default::default()
            },
            MemoryOptionalObservations {
                composition: MemoryCompositionObservations {
                    cached_bytes: OptionalObservation::present(gib(2), 10),
                    buffers_bytes: OptionalObservation::present(gib(1), 10),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        cx.notify();
    });
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    for sel in MEM_BASE_STAT_ROWS {
        let r = vcx
            .debug_bounds(sel)
            .unwrap_or_else(|| panic!("base memory stat {sel} must render"));
        assert!(r.size.height > px(10.0), "memory stat row collapsed: {r:?}");
    }
    assert!(
        vcx.debug_bounds("tm-perf-stat:8").is_none(),
        "the committed row must not render without commit-accounting data"
    );

    view.update(cx, |v, cx| {
        let m = &mut v.system_snapshot_mut_for_test().memory;
        let mut scalar = *m.scalar_observations();
        scalar.used_rate_mib_per_sec = ScalarObservation::available(12.5, 10);
        let mut optional = m.optional_observations().clone();
        optional.virtual_memory_commit = VirtualMemoryCommitObservations {
            committed_bytes: OptionalObservation::present(gib(9), 10),
            limit_bytes: OptionalObservation::present(gib(16), 10),
        };
        optional.compression = MemoryCompressionObservations {
            compressed_swap_used_bytes: OptionalObservation::present(mib(256), 10),
            compressed_swap_capacity_bytes: OptionalObservation::present(gib(2), 10),
            compressed_swap_memory_used_bytes: OptionalObservation::present(mib(128), 10),
            compressed_swap_cache_enabled: OptionalObservation::present(true, 10),
            ..Default::default()
        };
        m.apply_observations(scalar, optional);
        cx.notify();
    });
    vcx.update(|window, cx| window.draw(cx).clear());
    // committed, zram swap, zram RAM used (`mm_stat` `mem_used_total`),
    // zswap, usage rate — in that order.
    for sel in [
        "tm-perf-stat:8",
        "tm-perf-stat:9",
        "tm-perf-stat:10",
        "tm-perf-stat:11",
        "tm-perf-stat:12",
    ] {
        assert!(
            vcx.debug_bounds(sel).is_some(),
            "{sel} must render when its snapshot data exists"
        );
    }
    assert!(
        vcx.debug_bounds("tm-perf-stat:13").is_none(),
        "no stats row may render beyond the snapshot-derived set"
    );
}

/// Render-path assertion (后置): with memory data present, the page chrome —
/// the 26px title row and the fixed 280px stats column — must paint with sane
/// geometry, not collapse to zero.
#[gpui::test]
async fn memory_page_paints_title_and_stats_panel_geometry(cx: &mut TestAppContext) {
    let (win, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Performance;
        v.selected = SelectedDevice::Memory;
        v.system_snapshot_mut_for_test().memory = MemoryMetrics::from_observations(
            MemoryScalarObservations {
                total_bytes: ScalarObservation::available(gib(16), 10),
                used_bytes: ScalarObservation::available(gib(6), 10),
                swap_total_bytes: ScalarObservation::available(gib(2), 10),
                ..Default::default()
            },
            MemoryOptionalObservations {
                composition: MemoryCompositionObservations {
                    cached_bytes: OptionalObservation::present(gib(2), 10),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        cx.notify();
    });
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let title = vcx
        .debug_bounds("tm-perf-title")
        .expect("memory page must paint its title row");
    assert!(title.size.height > px(20.0), "title collapsed: {title:?}");
    assert!(title.size.width > px(0.0), "title has no width: {title:?}");
    let panel = vcx
        .debug_bounds("tm-perf-stats-panel")
        .expect("memory page must paint its stats column");
    let main_surface = vcx
        .debug_bounds("tm-perf-main-surface")
        .expect("the main Performance surface must remain addressable");
    let stats_surface = vcx
        .debug_bounds("tm-perf-stats-surface")
        .expect("the stats Performance surface must remain addressable");
    let page_frame = vcx
        .debug_bounds("tm-performance-page-frame")
        .expect("the Performance frame must remain addressable");
    assert_eq!(
        main_surface.right(),
        stats_surface.left(),
        "the split must use one bordered surface seam, not a transparent gap"
    );
    assert_eq!(
        stats_surface.right(),
        page_frame.right(),
        "the page-owned rail surface must reach the trailing page edge without an outer gutter"
    );
    assert!(
        panel.size.width > px(200.0) && panel.size.height > px(100.0),
        "stats panel geometry collapsed: {panel:?}"
    );
    assert!(
        vcx.debug_bounds("tm-memory-overview-card").is_some(),
        "memory page must elevate its summary and composition into a card"
    );
    let r0 = vcx
        .debug_bounds("tm-perf-stat:0")
        .expect("first stats row must render");
    assert!(r0.size.height > px(10.0), "stats row collapsed: {r0:?}");
}

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
            taskmanager_test_support::DiskMetricsFixtureBuilder::new()
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
                    taskmanager_test_support::DiskPartitionFixtureBuilder::new()
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
                    taskmanager_test_support::DiskPartitionFixtureBuilder::new()
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
    for (width, height) in [(720.0f32, 480.0f32), (1180.0, 780.0), (1900.0, 1344.0)] {
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
        if width == 720.0 {
            let partitions = vcx
                .debug_bounds("tm-disk-partitions")
                .expect("compact disk page must keep the partition panel addressable");
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

/// The device-page main column is one fixed viewport (the CPU-page
/// contract): even a partition-heavy disk page must compose into the window
/// — the headline graph compresses to its readable minimum and no second
/// scrolling body may appear between the sidebar and the pinned stats rail.
#[gpui::test]
async fn disk_page_composes_instead_of_scrolling(cx: &mut TestAppContext) {
    let (win, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Performance;
        v.selected = SelectedDevice::Disk(0);
        let partitions = (0..32)
            .map(|index| {
                taskmanager_test_support::DiskPartitionFixtureBuilder::new()
                    .device_id(format!("partition:fixture:fixture{index}"))
                    .parent_device_id("fixture".into())
                    .device_generation(DeviceGeneration::new(1))
                    .device_state(DeviceState::healthy(10))
                    .name(format!("fixture{index}"))
                    .mount_point(format!("/mnt/taskforest-compose-{index}"))
                    .fs_type("ext4".into())
                    .scalar_observations(DiskPartitionScalarObservations {
                        capacity_bytes: ScalarObservation::available(gib(10), 10),
                        used_bytes: ScalarObservation::available(gib(4), 10),
                        free_bytes: ScalarObservation::available(gib(6), 10),
                    })
                    .build()
            })
            .collect();
        v.system_snapshot_mut_for_test().disks = vec![
            taskmanager_test_support::DiskMetricsFixtureBuilder::new()
                .device_id("fixture".into())
                .name("fixture".into())
                .current_capacity_bytes(gib(320))
                .current_available_bytes(gib(192))
                .partitions(partitions)
                .build(),
        ];
        cx.notify();
    });
    cx.simulate_window_resize(win.into(), size(px(720.0), px(480.0)));
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    assert!(
        vcx.debug_bounds("tm-perf-left-scroll").is_none()
            && vcx.debug_bounds("tm-perf-left-scrollbar").is_none(),
        "a device page must not mount a scrolling main column beside the stats rail"
    );
    let viewport = vcx
        .debug_bounds("tm-perf-main-viewport")
        .expect("the fixed main viewport must render");
    assert!(
        f32::from(viewport.bottom()) <= 480.5,
        "the main viewport must stay inside the window: {viewport:?}"
    );
    let graph = vcx
        .debug_bounds("tm-perf-chart-card:main-graph")
        .expect("the shared main graph card must render");
    assert!(
        graph.size.height >= px(180.0),
        "at the product minimum window the headline card must still hold its tier floor: {graph:?}"
    );
    let panel = vcx
        .debug_bounds("tm-perf-stats-panel")
        .expect("the width-aware budget keeps the stats rail pinned at the minimum size");
    assert!(
        f32::from(panel.origin.x) >= -0.5 && f32::from(panel.origin.x + panel.size.width) <= 720.5,
        "the pinned stats panel must remain inside the viewport: {panel:?}"
    );
}

/// The Network page uses the same shared device-page graph helper as Disk, but
/// remains a separate render path because its selected-device projection and
/// stats rows are different.  Keep a compact viewport assertion here so a
/// future network-specific change cannot reintroduce the zero-height chart.
#[gpui::test]
async fn network_page_keeps_shared_main_graph_readable_in_compact_view(cx: &mut TestAppContext) {
    let (win, view) = wrapped_root(cx);
    cx.simulate_window_resize(win.into(), size(px(720.0), px(480.0)));
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Performance;
        v.system_snapshot_mut_for_test().networks = vec![
            taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
                .device_id("fixture:nic0".into())
                .interface_name("nic0".into())
                .ipv4_addr(Some("192.0.2.10".into()))
                .build(),
        ];
        v.selected = SelectedDevice::Nic(0);
        cx.notify();
    });
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let graph = vcx
        .debug_bounds("tm-graph:main-graph")
        .expect("network page must paint the shared main graph");
    assert!(
        vcx.debug_bounds("tm-perf-left-scrollbar").is_none(),
        "the network main column is a fixed viewport and must not mount a scrollbar"
    );
    assert!(
        vcx.debug_bounds("tm-perf-stats-scrollbar").is_some(),
        "the network statistics column must keep its vertical rail mounted"
    );
    assert!(
        graph.size.height >= px(220.0),
        "compact network graph must consume the available detail height: {graph:?}"
    );
    assert!(
        f32::from(graph.origin.x) >= -0.5
            && f32::from(graph.origin.x + graph.size.width) <= 720.5
            && f32::from(graph.origin.y) >= -0.5
            && f32::from(graph.origin.y + graph.size.height) <= 480.5,
        "compact network graph must remain inside the viewport: {graph:?}"
    );

    // A sparse page must continue to use newly available height instead of
    // keeping the historical minimum-height canvas and leaving a blank lower
    // half. The shared fill-scroll contract is observable at the wide
    // breakpoint without hard-coding a pixel height into the renderer.
    cx.simulate_window_resize(win.into(), size(px(1180.0), px(780.0)));
    draw(cx, win);
    let wide_graph = vcx
        .debug_bounds("tm-graph:main-graph")
        .expect("wide network page must keep the shared main graph");
    assert!(
        wide_graph.size.height >= px(360.0),
        "wide network graph must expand into the available detail height: {wide_graph:?}"
    );
    assert!(
        wide_graph.size.width >= px(500.0),
        "wide network graph must consume the available left-column width: {wide_graph:?}"
    );
}

#[gpui::test]
async fn mc01_dynamic_history_case_battery_and_fan_pages_project_optional_upstream_trend_graphs(
    cx: &mut TestAppContext,
) {
    let (win, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Performance;

        let battery = with_battery_scalars(
            {
                let mut battery =
                    BatteryInfo::new("power-supply:fixture-battery", DeviceState::healthy(100));
                battery.device_generation = DeviceGeneration::new(1);
                battery.status = "Discharging".into();
                battery
            },
            100,
            78,
            14.2,
        );
        let power = PowerSupplySnapshot {
            timestamp_ms: 100,
            batteries: vec![battery],
            ..Default::default()
        };
        v.telemetry_ingestor
            .ingest_correlated_power_supplies(
                CorrelatedTelemetryStamp::from_accepted_event(1, 110)
                    .expect("non-zero test event revision"),
                &power,
            )
            .expect("fixture power history should be accepted");

        let device_id = DeviceId::new("hwmon:fixture-fan");
        let fan = sensor_reading(
            device_id.clone(),
            "hwmon:fixture-fan:fan1_input",
            "CPU fan",
            SensorDescriptor::fan_speed(SensorScale::IDENTITY),
            SensorMagnitude::Unsigned(1_420),
            100,
            1,
        );
        let temperature = sensor_reading(
            device_id,
            "hwmon:fixture-fan:temp1_input",
            "CPU package",
            SensorDescriptor::temperature(SensorScale::IDENTITY),
            SensorMagnitude::Decimal(48.5),
            100,
            1,
        );
        let sensors = SensorCenterSnapshot {
            timestamp_ms: 100,
            readings: vec![fan, temperature],
            ..Default::default()
        };
        v.replace_dynamic_devices_for_test(sensors.clone(), power.clone());
        v.telemetry_ingestor
            .ingest_correlated_sensors(
                CorrelatedTelemetryStamp::from_accepted_event(1, 110)
                    .expect("non-zero test event revision"),
                &sensors,
            )
            .expect("fixture sensor history should be accepted");
        v.selected = SelectedDevice::Battery(0);
        cx.notify();
    });
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    assert!(
        vcx.debug_bounds("tm-perf-secondary-graph:battery-power-graph")
            .is_some(),
        "battery power history must render when its typed channel is measured"
    );
    let battery_graph = vcx
        .debug_bounds("tm-graph:battery-power-graph")
        .expect("battery power graph must paint its chart canvas");
    assert!(
        battery_graph.size.height >= px(120.0),
        "battery power graph must retain a readable height: {battery_graph:?}"
    );

    view.update(cx, |v, cx| {
        v.selected = SelectedDevice::Fan(0);
        cx.notify();
    });
    vcx.update(|window, cx| window.draw(cx).clear());
    assert!(
        vcx.debug_bounds("tm-perf-secondary-graph:fan-temperature-graph")
            .is_some(),
        "fan temperature history must render when its typed channel is measured"
    );
    let fan_graph = vcx
        .debug_bounds("tm-graph:fan-temperature-graph")
        .expect("fan temperature graph must paint its chart canvas");
    assert!(
        fan_graph.size.height >= px(120.0),
        "fan temperature graph must retain a readable height: {fan_graph:?}"
    );
}

/// Battery and Fan are hot-pluggable domains. A reappearing device keeps its
/// stable ID but receives a new generation, so the selected page must project
/// only the new generation's history rather than drawing a stale pre-detach
/// trace.
#[gpui::test]
async fn mc01_generation_recovery_case_battery_and_fan_history_restarts_at_a_new_device_generation(
    cx: &mut TestAppContext,
) {
    use crate::gpui_app::history_samples::{battery_power_samples, fan_temperature_samples};

    let (win, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Performance;
        let battery = with_battery_scalars(
            {
                let mut battery =
                    BatteryInfo::new("power-supply:generation-battery", DeviceState::healthy(100));
                battery.device_generation = DeviceGeneration::new(1);
                battery.status = "Discharging".into();
                battery
            },
            1,
            70,
            10.0,
        );
        let power = PowerSupplySnapshot {
            timestamp_ms: 100,
            batteries: vec![battery],
            ..Default::default()
        };
        v.telemetry_ingestor
            .ingest_correlated_power_supplies(
                CorrelatedTelemetryStamp::from_accepted_event(1, 110)
                    .expect("generation-one power event must be accepted"),
                &power,
            )
            .expect("generation-one power history must be accepted");

        let fan_device = DeviceId::new("hwmon:generation-fan");
        let sensors = SensorCenterSnapshot {
            timestamp_ms: 100,
            readings: vec![
                sensor_reading(
                    fan_device.clone(),
                    "hwmon:generation-fan:fan1_input",
                    "Generation fan",
                    SensorDescriptor::fan_speed(SensorScale::IDENTITY),
                    SensorMagnitude::Unsigned(1_100),
                    100,
                    1,
                ),
                sensor_reading(
                    fan_device,
                    "hwmon:generation-fan:temp1_input",
                    "Generation temperature",
                    SensorDescriptor::temperature(SensorScale::IDENTITY),
                    SensorMagnitude::Decimal(44.0),
                    100,
                    1,
                ),
            ],
            ..Default::default()
        };
        v.replace_dynamic_devices_for_test(sensors.clone(), power.clone());
        v.telemetry_ingestor
            .ingest_correlated_sensors(
                CorrelatedTelemetryStamp::from_accepted_event(1, 110)
                    .expect("generation-one sensor event must be accepted"),
                &sensors,
            )
            .expect("generation-one sensor history must be accepted");
        v.selected = SelectedDevice::Battery(0);
        cx.notify();
    });
    draw(cx, win);

    view.read_with(cx, |v, _cx| {
        assert_eq!(
            battery_power_samples(
                &v.telemetry.dynamic_history,
                "power-supply:generation-battery",
                DeviceGeneration::new(1),
            ),
            std::rc::Rc::from(vec![10.0].as_slice())
        );
        assert_eq!(
            fan_temperature_samples(
                &v.telemetry.dynamic_history,
                "hwmon:generation-fan:temp1_input",
                DeviceGeneration::new(1),
            ),
            std::rc::Rc::from(vec![44.0].as_slice())
        );
    });

    view.update(cx, |v, cx| {
        let battery = with_battery_scalars(
            {
                let mut battery =
                    BatteryInfo::new("power-supply:generation-battery", DeviceState::healthy(200));
                battery.device_generation = DeviceGeneration::new(2);
                battery.status = "Charging".into();
                battery
            },
            2,
            71,
            22.0,
        );
        let power = PowerSupplySnapshot {
            timestamp_ms: 200,
            batteries: vec![battery],
            ..Default::default()
        };
        v.telemetry_ingestor
            .ingest_correlated_power_supplies(
                CorrelatedTelemetryStamp::from_accepted_event(2, 210)
                    .expect("generation-two power event must be accepted"),
                &power,
            )
            .expect("generation-two power history must be accepted");

        let fan_device = DeviceId::new("hwmon:generation-fan");
        let sensors = SensorCenterSnapshot {
            timestamp_ms: 200,
            readings: vec![
                sensor_reading(
                    fan_device.clone(),
                    "hwmon:generation-fan:fan1_input",
                    "Generation fan",
                    SensorDescriptor::fan_speed(SensorScale::IDENTITY),
                    SensorMagnitude::Unsigned(1_200),
                    200,
                    2,
                ),
                sensor_reading(
                    fan_device,
                    "hwmon:generation-fan:temp1_input",
                    "Generation temperature",
                    SensorDescriptor::temperature(SensorScale::IDENTITY),
                    SensorMagnitude::Decimal(55.0),
                    200,
                    2,
                ),
            ],
            ..Default::default()
        };
        v.replace_dynamic_devices_for_test(sensors.clone(), power.clone());
        v.telemetry_ingestor
            .ingest_correlated_sensors(
                CorrelatedTelemetryStamp::from_accepted_event(2, 210)
                    .expect("generation-two sensor event must be accepted"),
                &sensors,
            )
            .expect("generation-two sensor history must be accepted");
        cx.notify();
    });
    draw(cx, win);

    view.read_with(cx, |v, _cx| {
        assert_eq!(
            battery_power_samples(
                &v.telemetry.dynamic_history,
                "power-supply:generation-battery",
                DeviceGeneration::new(2),
            ),
            std::rc::Rc::from(vec![22.0].as_slice())
        );
        assert_eq!(
            fan_temperature_samples(
                &v.telemetry.dynamic_history,
                "hwmon:generation-fan:temp1_input",
                DeviceGeneration::new(2),
            ),
            std::rc::Rc::from(vec![55.0].as_slice())
        );
    });
}

/// Standard GPU pages expose every engine without a selector. Compact pages
/// switch to one readable aggregate graph instead of crushing the engine grid.
#[gpui::test]
async fn mc04_gpu_layout_case_gpu_page_adapts_complete_engine_inventory_to_available_space(
    cx: &mut TestAppContext,
) {
    let (win, view) = wrapped_root(cx);
    cx.simulate_window_resize(win.into(), size(px(1180.0), px(780.0)));
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Performance;
        v.selected = SelectedDevice::Gpu(0);
        let device_id = "gpu:fixture:engine-grid";
        let mut gpu = GpuMetrics::new(device_id, "Capture GPU");
        gpu.marketing_name =
            Some("A production-length graphics product name that must stay on one line".into());
        gpu.device_generation = DeviceGeneration::new(1);
        gpu.device_state = DeviceState::healthy(10);
        gpu.apply_scalar_observations(GpuScalarObservations {
            utilization_pct: ScalarObservation::available(42.0, 10),
            memory_used_bytes: ScalarObservation::available(3 * 1024, 10),
            memory_total_bytes: ScalarObservation::available(8 * 1024, 10),
            dedicated_vram_used_bytes: ScalarObservation::available(3 * 1024 * 1024 * 1024, 10),
            dedicated_vram_total_bytes: ScalarObservation::available(8 * 1024 * 1024 * 1024, 10),
            shared_vram_used_bytes: ScalarObservation::available(640 * 1024 * 1024, 10),
            shared_vram_total_bytes: ScalarObservation::available(16 * 1024 * 1024 * 1024, 10),
            frequency_mhz: ScalarObservation::available(1_800, 10),
            ..Default::default()
        });
        gpu.engines = vec![
            GpuEngine {
                name: "3D".into(),
                kind: GpuEngineKind::Render,
                usage_pct: 25.0,
            },
            GpuEngine {
                name: "Video Decode".into(),
                kind: GpuEngineKind::VideoDecode,
                usage_pct: 7.0,
            },
        ];
        v.system_snapshot_mut_for_test().gpu = vec![gpu.clone()];
        cx.notify();
    });
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    assert!(
        vcx.debug_bounds("tm-perf-gpu-engine-grid").is_some(),
        "standard GPU page must paint the complete engine inventory"
    );
    for (engine, selector) in [
        ("3D", "tm-perf-gpu-engine:3D"),
        ("Video Decode", "tm-perf-gpu-engine:Video Decode"),
    ] {
        assert!(
            vcx.debug_bounds(selector).is_some(),
            "standard GPU page must retain the {engine} engine card"
        );
    }
    assert!(
        vcx.debug_bounds("tm-graph:main-graph").is_none(),
        "standard multi-engine GPU page must own exactly one engine-grid main surface"
    );
    assert!(
        vcx.debug_bounds("tm-perf-aggregate-graph-summary")
            .is_none(),
        "standard multi-engine GPU page must not label aggregate statistics as engine statistics"
    );
    let stats_surface = vcx
        .debug_bounds("tm-perf-stats-surface")
        .expect("GPU page must paint the stats surface");
    let contains = |outer: gpui::Bounds<gpui::Pixels>, inner: gpui::Bounds<gpui::Pixels>| {
        inner.left() >= outer.left()
            && inner.right() <= outer.right()
            && inner.top() >= outer.top()
            && inner.bottom() <= outer.bottom()
    };
    let product = vcx
        .debug_bounds("tm-perf-stat-value:2")
        .expect("long GPU product name must remain a selectable readout");
    assert!(
        contains(stats_surface, product) && product.size.height <= px(24.0),
        "long production GPU identity must stay on one bounded line: {product:?}"
    );
    let vram_block = vcx
        .debug_bounds("tm-gpu-vram-composition")
        .expect("complete split VRAM facts paint the composition block");
    assert!(
        contains(stats_surface, vram_block),
        "VRAM composition must remain inside the 280px stats surface: {vram_block:?}"
    );
    for (row_name, selector) in [
        ("dedicated", "tm-gpu-vram-row:dedicated"),
        ("shared", "tm-gpu-vram-row:shared"),
        ("total", "tm-gpu-vram-row:total"),
    ] {
        let row = vcx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("missing vertical VRAM summary row {row_name}"));
        assert!(
            contains(vram_block, row) && row.size.height <= px(24.0),
            "VRAM {row_name} label/value must remain one bounded line: {row:?}"
        );
    }
    assert_ne!(
        crate::i18n::t("gpu.vram_total"),
        "gpu.vram_total",
        "the total row must use an existing localized product key"
    );

    let gpu = view.read_with(cx, |view, _| view.system_snapshot().gpu[0].clone());
    let (compact_win, compact_view) = wrapped_root(cx);
    cx.simulate_window_resize(compact_win.into(), size(px(720.0), px(480.0)));
    compact_view.update(cx, |view, cx| {
        view.mark_telemetry_frame_ready();
        view.page = TopPage::Performance;
        view.selected = SelectedDevice::Gpu(0);
        view.system_snapshot_mut_for_test().gpu = vec![gpu];
        cx.notify();
    });
    draw(cx, compact_win);
    let mut vcx = VisualTestContext::from_window(compact_win.into(), cx);
    assert!(
        vcx.debug_bounds("tm-perf-gpu-engine-grid").is_none(),
        "compact GPU page must not compress the complete engine inventory"
    );
    assert!(
        vcx.debug_bounds("tm-perf-gpu-primary-engine-summary")
            .is_none(),
        "compact GPU page must not retain a hidden engine summary"
    );
    assert!(
        vcx.debug_bounds("tm-perf-left-scrollbar").is_none(),
        "compact aggregate chart owns the viewport and must not expose a central scrollbar"
    );
    let aggregate = vcx
        .debug_bounds("tm-graph:main-graph")
        .expect("compact GPU page must render the aggregate utilization graph");
    assert!(
        aggregate.size.width > px(100.0) && aggregate.size.height > px(100.0),
        "compact aggregate GPU graph must remain readable: {aggregate:?}"
    );
}

#[gpui::test]
async fn mc01_dynamic_readout_case_battery_and_fan_pages_paint_typed_dynamic_device_data(
    cx: &mut TestAppContext,
) {
    let (win, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Performance;
        let battery = with_battery_scalars(
            {
                let mut battery = BatteryInfo::new("power-supply:BAT0", DeviceState::healthy(10));
                battery.display_name = "Internal battery".into();
                battery.device_generation = DeviceGeneration::new(1);
                battery
            },
            10,
            73,
            12.5,
        );
        let battery_snapshot = PowerSupplySnapshot {
            timestamp_ms: 10,
            batteries: vec![battery.clone()],
            ..Default::default()
        };
        v.replace_dynamic_devices_for_test(
            SensorCenterSnapshot::default(),
            battery_snapshot.clone(),
        );
        v.telemetry_ingestor
            .ingest_correlated_power_supplies(
                taskmanager_telemetry_store::CorrelatedTelemetryStamp::from_accepted_event(1, 20)
                    .expect("fixture revision is non-zero"),
                &battery_snapshot,
            )
            .expect("battery fixture enters dynamic history");
        v.selected = SelectedDevice::Battery(0);
        cx.notify();
    });
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    assert!(vcx.debug_bounds("tm-perf-title").is_some());
    assert!(vcx.debug_bounds("tm-perf-stat:0").is_some());

    view.update(cx, |v, cx| {
        let fan = sensor_reading(
            DeviceId::new("hwmon:pwm"),
            "hwmon:pwm:fan1_input",
            "CPU fan",
            SensorDescriptor::fan_speed(SensorScale::IDENTITY),
            SensorMagnitude::Unsigned(1_380),
            30,
            2,
        );
        let fan_snapshot = SensorCenterSnapshot {
            timestamp_ms: 30,
            readings: vec![fan],
            ..Default::default()
        };
        let power_supplies = v.power_supplies().clone();
        v.replace_dynamic_devices_for_test(fan_snapshot.clone(), power_supplies);
        v.telemetry_ingestor
            .ingest_correlated_sensors(
                taskmanager_telemetry_store::CorrelatedTelemetryStamp::from_accepted_event(1, 40)
                    .expect("fixture revision is non-zero"),
                &fan_snapshot,
            )
            .expect("fan fixture enters dynamic history");
        v.selected = SelectedDevice::Fan(0);
        cx.notify();
    });
    vcx.update(|window, cx| window.draw(cx).clear());
    assert!(vcx.debug_bounds("tm-perf-title").is_some());
    assert!(vcx.debug_bounds("tm-perf-stat:0").is_some());
}
