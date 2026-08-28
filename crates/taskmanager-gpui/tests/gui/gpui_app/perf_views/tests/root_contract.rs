//! One-composition-root guard: every Performance device page assembles
//! through the shared `perf_page` root and the chart-tier system. A new page
//! that hand-rolls its own viewport, title row, or card contract fails here
//! before it can drift.

use super::*;
use crate::core::metrics::SystemSnapshot;

/// The shared chrome every device page must paint: the semantic title row and
/// the ONE fixed main viewport. No page may mount its own scrolling main
/// column beside the statistics rail.
fn assert_shared_root_chrome(vcx: &mut VisualTestContext, page: &str) {
    let title = vcx
        .debug_bounds("tm-perf-title")
        .unwrap_or_else(|| panic!("{page} must paint the shared title row"));
    assert!(
        title.size.height > px(20.0),
        "{page} title collapsed: {title:?}"
    );
    let viewport = vcx
        .debug_bounds("tm-perf-main-viewport")
        .unwrap_or_else(|| panic!("{page} must compose through the shared main viewport"));
    assert!(
        viewport.size.width > px(100.0) && viewport.size.height > px(100.0),
        "{page} main viewport collapsed: {viewport:?}"
    );
    assert!(
        vcx.debug_bounds("tm-perf-left-scroll").is_none()
            && vcx.debug_bounds("tm-perf-left-scrollbar").is_none(),
        "{page} must not mount a page-local scrolling main column"
    );
}

fn contract_snapshot() -> SystemSnapshot {
    SystemSnapshot {
        cpu: CpuMetrics::from_observations(CpuScalarObservations {
            global_usage_pct: ScalarObservation::available(42.0, 10),
            core_usage_group: ScalarObservationGroup::available(vec![12.0, 55.0], 10),
            ..Default::default()
        }),
        memory: MemoryMetrics::from_observations(
            MemoryScalarObservations {
                total_bytes: ScalarObservation::available(gib(16), 10),
                used_bytes: ScalarObservation::available(gib(6), 10),
                swap_total_bytes: ScalarObservation::available(gib(2), 10),
                swap_used_bytes: ScalarObservation::available(mib(512), 10),
                ..Default::default()
            },
            MemoryOptionalObservations::default(),
        ),
        disks: vec![
            taskmanager_test_support::DiskMetricsFixtureBuilder::new()
                .device_id("contract-disk".into())
                .name("contract0n1".into())
                .disk_type("NVMe SSD".into())
                .fs_type("ext4".into())
                .build(),
        ],
        networks: vec![
            taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
                .device_id("contract-nic".into())
                .interface_name("contract0".into())
                .build(),
        ],
        gpu: vec![{
            let mut gpu = GpuMetrics::new("contract-gpu", "Contract GPU");
            gpu.device_state = DeviceState::healthy(10);
            gpu
        }],
        ..SystemSnapshot::default()
    }
}

/// Every selectable device page routes through one composition root and one
/// headline-tier chart identity. The page-specific headline selector table is
/// deliberate: a page that swaps in a bespoke chart assembly no longer
/// matches its row and this guard fails.
#[gpui::test]
async fn every_device_page_composes_through_the_shared_root(cx: &mut TestAppContext) {
    let (win, view) = wrapped_root(cx);
    cx.simulate_window_resize(win.into(), size(px(1180.0), px(780.0)));
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Performance;
        *v.system_snapshot_mut_for_test() = contract_snapshot();

        let battery = with_battery_scalars(
            {
                let mut battery =
                    BatteryInfo::new("power-supply:contract-battery", DeviceState::healthy(10));
                battery.device_generation = DeviceGeneration::new(1);
                battery.display_name = "Contract battery".into();
                battery
            },
            100,
            78,
            14.2,
        );
        let fan = sensor_reading(
            DeviceId::new("hwmon:contract-fan"),
            "hwmon:contract-fan:fan1_input",
            "Contract fan",
            SensorDescriptor::fan_speed(SensorScale::IDENTITY),
            SensorMagnitude::Unsigned(1_400),
            100,
            1,
        );
        v.replace_dynamic_devices_for_test(
            SensorCenterSnapshot {
                timestamp_ms: 100,
                readings: vec![fan],
                ..Default::default()
            },
            PowerSupplySnapshot {
                timestamp_ms: 100,
                batteries: vec![battery],
                ..Default::default()
            },
        );
        cx.notify();
    });

    for (page, device, headline_selector) in [
        (
            "CPU",
            SelectedDevice::Cpu,
            "tm-perf-chart-card:cpu-headline-graph",
        ),
        (
            "Memory",
            SelectedDevice::Memory,
            "tm-perf-chart-card:mem-graph",
        ),
        (
            "Disk",
            SelectedDevice::Disk(0),
            "tm-perf-chart-card:main-graph",
        ),
        (
            "Network",
            SelectedDevice::Nic(0),
            "tm-perf-chart-card:main-graph",
        ),
        (
            "GPU",
            SelectedDevice::Gpu(0),
            "tm-perf-chart-card:main-graph",
        ),
        (
            "Battery",
            SelectedDevice::Battery(0),
            "tm-perf-chart-card:main-graph",
        ),
        (
            "Fan",
            SelectedDevice::Fan(0),
            "tm-perf-chart-card:main-graph",
        ),
    ] {
        view.update(cx, |v, cx| {
            v.selected = device;
            cx.notify();
        });
        draw(cx, win);
        let mut vcx = VisualTestContext::from_window(win.into(), cx);
        assert_shared_root_chrome(&mut vcx, page);
        let headline = vcx
            .debug_bounds(headline_selector)
            .unwrap_or_else(|| panic!("{page} must render its headline through the chart tiers"));
        assert!(
            headline.size.height >= px(180.0),
            "{page} headline card must keep the tier floor: {headline:?}"
        );
        drop(vcx);
    }
}

/// The typed vertical ladder degrades in order and never touches the
/// headline floor: as the window shrinks, the per-core matrix goes first
/// (Charts rung), then the header band and summary rows (Floor rung) — the
/// headline card keeps its 180px floor through every rung.
///
/// Each rung runs on a FRESH window (the established convention): debug
/// selector entries persist across frames within one window, so absence
/// assertions cannot ride a resized window that painted the element earlier.
#[gpui::test]
async fn vertical_runway_degrades_in_order_before_the_headline_floor(cx: &mut TestAppContext) {
    fn seed_cpu(v: &mut RootView) {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Performance;
        v.selected = SelectedDevice::Cpu;
        v.system_snapshot_mut_for_test().cpu =
            CpuMetrics::from_observations(CpuScalarObservations {
                global_usage_pct: ScalarObservation::available(42.0, 10),
                core_usage_group: ScalarObservationGroup::available(vec![12.0, 55.0], 10),
                ..Default::default()
            });
        // Two measured aggregate samples so the headline's summary row has a
        // finite window to summarize, plus the accepted-CPU-outcome signal
        // the live platform-batch apply path emits so the generation-keyed
        // history cache rebuilds on render.
        for (revision, usage) in [(1_u64, 40.0_f32), (2, 42.0)] {
            let stamp = CorrelatedTelemetryStamp::from_accepted_event(revision, revision * 10 + 1)
                .expect("fixture revision is non-zero");
            let observation = crate::core::CpuTelemetryObservation::current(
                CpuMetrics::from_observations(CpuScalarObservations {
                    global_usage_pct: ScalarObservation::available(usage, revision * 10),
                    ..Default::default()
                }),
                revision * 10,
                Vec::new(),
            );
            v.telemetry_ingestor
                .ingest_correlated_cpu(stamp, &observation)
                .expect("cpu observation ingests");
        }
        v.cpu_core_history.bump();
    }

    // Charts rung (1180x780): matrix, readout band, summary, headline — all
    // compose.
    {
        let (win, view) = wrapped_root(cx);
        cx.simulate_window_resize(win.into(), size(px(1180.0), px(780.0)));
        view.update(cx, |v, cx| {
            seed_cpu(v);
            cx.notify();
        });
        draw(cx, win);
        let mut vcx = VisualTestContext::from_window(win.into(), cx);
        assert!(
            vcx.debug_bounds("tm-cpu-per-core-matrix").is_some(),
            "the Charts rung carries the per-core matrix"
        );
        assert!(vcx.debug_bounds("tm-cpu-readouts").is_some());
        assert!(
            vcx.debug_bounds("tm-perf-chart-summary:cpu-headline-graph")
                .is_some(),
            "the Charts rung carries the summary row"
        );
        assert!(
            vcx.debug_bounds("tm-perf-chart-card:cpu-headline-graph")
                .expect("headline card")
                .size
                .height
                >= px(180.0)
        );
    }

    // Core rung (1180x560, content ≈ 468: between the core floor 380 and
    // the Charts threshold 640): matrix drops, header band and summary stay.
    {
        let (win, view) = wrapped_root(cx);
        cx.simulate_window_resize(win.into(), size(px(1180.0), px(560.0)));
        view.update(cx, |v, cx| {
            seed_cpu(v);
            cx.notify();
        });
        draw(cx, win);
        let mut vcx = VisualTestContext::from_window(win.into(), cx);
        assert!(
            vcx.debug_bounds("tm-cpu-per-core-matrix").is_none(),
            "the Core rung drops the chart inventory before the core stack"
        );
        assert!(
            vcx.debug_bounds("tm-cpu-readouts").is_some(),
            "the Core rung keeps the header band"
        );
        assert!(
            vcx.debug_bounds("tm-perf-chart-summary:cpu-headline-graph")
                .is_some(),
            "the Core rung keeps the summary row"
        );
        assert!(
            vcx.debug_bounds("tm-perf-chart-card:cpu-headline-graph")
                .expect("headline card")
                .size
                .height
                >= px(180.0)
        );
    }

    // Floor rung (1180x340, content ≈ 228: below the core floor 380): the
    // header band and summary row drop EXPLICITLY — and the headline card
    // still holds its tier floor. This is the ladder's promise: ordered
    // degradation, never an incoherent squeeze.
    {
        let (win, view) = wrapped_root(cx);
        cx.simulate_window_resize(win.into(), size(px(1180.0), px(340.0)));
        view.update(cx, |v, cx| {
            seed_cpu(v);
            cx.notify();
        });
        draw(cx, win);
        let mut vcx = VisualTestContext::from_window(win.into(), cx);
        assert!(
            vcx.debug_bounds("tm-cpu-readouts").is_none(),
            "the Floor rung drops the header band"
        );
        assert!(
            vcx.debug_bounds("tm-perf-chart-summary:cpu-headline-graph")
                .is_none(),
            "the Floor rung drops the summary row"
        );
        let card = vcx
            .debug_bounds("tm-perf-chart-card:cpu-headline-graph")
            .expect("the headline card survives every rung");
        assert!(
            card.size.height >= px(180.0),
            "the headline tier floor holds at the Floor rung: {card:?}"
        );
        assert!(vcx.debug_bounds("tm-perf-title").is_some());
    }
}

/// The Disk page's non-chart primary fact (capacity) survives every
/// vertical rung: the partition PANEL degrades away below the Core rung,
/// but the one-line vital capacity stays beside the headline chart even at
/// the Floor rung — the page never collapses to a chart-only surface.
#[gpui::test]
async fn disk_page_keeps_its_capacity_fact_through_every_vertical_rung(cx: &mut TestAppContext) {
    fn seed_disk(v: &mut RootView) {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Performance;
        v.selected = SelectedDevice::Disk(0);
        v.system_snapshot_mut_for_test().disks = vec![
            taskmanager_test_support::DiskMetricsFixtureBuilder::new()
                .device_id("vital-disk".into())
                .name("vital0n1".into())
                .disk_type("NVMe SSD".into())
                .fs_type("ext4".into())
                .current_capacity_bytes(gib(2000))
                .current_available_bytes(gib(1200))
                .partitions(vec![
                    taskmanager_test_support::DiskPartitionFixtureBuilder::new()
                        .device_id("partition:vital:vital0n1p1".into())
                        .parent_device_id("vital-disk".into())
                        .device_generation(DeviceGeneration::new(1))
                        .device_state(DeviceState::healthy(10))
                        .name("vital0n1p1".into())
                        .mount_point("/".into())
                        .fs_type("ext4".into())
                        .build(),
                    taskmanager_test_support::DiskPartitionFixtureBuilder::new()
                        .device_id("partition:vital:vital0n1p2".into())
                        .parent_device_id("vital-disk".into())
                        .device_generation(DeviceGeneration::new(1))
                        .device_state(DeviceState::healthy(10))
                        .name("vital0n1p2".into())
                        .build(),
                ])
                .build(),
        ];
    }

    // Charts rung: panel + vital + chart all compose.
    {
        let (win, view) = wrapped_root(cx);
        cx.simulate_window_resize(win.into(), size(px(1180.0), px(780.0)));
        view.update(cx, |v, cx| {
            seed_disk(v);
            cx.notify();
        });
        draw(cx, win);
        let mut vcx = VisualTestContext::from_window(win.into(), cx);
        assert!(vcx.debug_bounds("tm-perf-vital-line").is_some());
        assert!(
            vcx.debug_bounds("tm-disk-partitions").is_some(),
            "the Charts rung carries the partition panel"
        );
        assert!(
            vcx.debug_bounds("tm-perf-chart-card:main-graph")
                .expect("headline card")
                .size
                .height
                >= px(180.0)
        );
    }

    // Floor rung: the panel and every secondary chart are gone, but the
    // capacity vital stays — never a chart-only disk page.
    {
        let (win, view) = wrapped_root(cx);
        cx.simulate_window_resize(win.into(), size(px(1180.0), px(340.0)));
        view.update(cx, |v, cx| {
            seed_disk(v);
            cx.notify();
        });
        draw(cx, win);
        let mut vcx = VisualTestContext::from_window(win.into(), cx);
        let vital = vcx
            .debug_bounds("tm-perf-vital-line")
            .expect("the capacity vital survives the Floor rung");
        assert!(
            vital.size.width > px(40.0) && vital.size.height > px(10.0),
            "the vital line must paint readable: {vital:?}"
        );
        assert!(
            vcx.debug_bounds("tm-disk-partitions").is_none(),
            "the partition panel yields at the Floor rung"
        );
        assert!(
            vcx.debug_bounds("tm-perf-chart-card:main-graph")
                .expect("headline card")
                .size
                .height
                >= px(180.0)
        );
    }
}

/// The tier height contract: headline cards hold their 180px floor while
/// secondary charts hold 140px, on one page that carries both tiers.
#[gpui::test]
async fn chart_tiers_hold_their_height_floors(cx: &mut TestAppContext) {
    let (win, view) = wrapped_root(cx);
    cx.simulate_window_resize(win.into(), size(px(1180.0), px(780.0)));
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Performance;
        *v.system_snapshot_mut_for_test() = contract_snapshot();

        let battery = with_battery_scalars(
            {
                let mut battery =
                    BatteryInfo::new("power-supply:tiers-battery", DeviceState::healthy(10));
                battery.device_generation = DeviceGeneration::new(1);
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
        v.replace_dynamic_devices_for_test(SensorCenterSnapshot::default(), power);
        cx.notify();
    });

    // Memory: two headline charts (memory + swap) split the viewport and each
    // holds the headline floor.
    view.update(cx, |v, cx| {
        v.selected = SelectedDevice::Memory;
        cx.notify();
    });
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    for selector in [
        "tm-perf-chart-card:mem-graph",
        "tm-perf-chart-card:swap-graph",
    ] {
        let card = vcx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("memory twin headline {selector} must render"));
        assert!(
            card.size.height >= px(180.0),
            "headline tier floor breached for {selector}: {card:?}"
        );
    }
    drop(vcx);

    // Battery: the power channel renders on the secondary tier and holds its
    // own floor.
    view.update(cx, |v, cx| {
        v.selected = SelectedDevice::Battery(0);
        cx.notify();
    });
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let secondary = vcx
        .debug_bounds("tm-perf-secondary-graph:battery-power-graph")
        .expect("battery power secondary chart must render");
    assert!(
        secondary.size.height >= px(140.0),
        "secondary tier floor breached: {secondary:?}"
    );
}
