//! Performance-page WIDTH-contract guard: the horizontal degradation ladder
//! (Pinned → Stacked → Hidden) must hold as painted geometry, not just as
//! budget arithmetic. Across the whole width matrix the statistics rail is
//! either absent (Hidden), full-width stacked, or at least
//! `PERFORMANCE_STATS_MIN_WIDTH` wide with its right edge inside the window —
//! never a clipped sliver, never squeezed below its floor.
//!
//! This file proves GEOMETRY only: the headless test renderer does not shape
//! real text, so text-measure regressions (the nowrap measure-cache "…"
//! poisoning seen on the vital line) are out of scope here. The pixel-level
//! gate for truncation is the niri screenshot matrix
//! (`scripts/capture_scenarios.tsv`, perf-* rows, via `scripts/capture-niri.sh`).

use super::*;
use crate::gpui_app::perf_views::PERF_MAIN_VIEWPORT_SELECTOR;
use crate::gpui_app::root::responsive::PERFORMANCE_STATS_MIN_WIDTH;
use taskmanager_core::core::metrics::SystemSnapshot;

fn contract_page_snapshot() -> SystemSnapshot {
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
        networks: vec![
            taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
                .device_id("width-nic".into())
                .interface_name("width0".into())
                .build(),
        ],
        ..SystemSnapshot::default()
    }
}

fn draw_perf_page_at(
    cx: &mut TestAppContext,
    device: SelectedDevice,
    width: f32,
    height: f32,
) -> VisualTestContext {
    let (win, view) = wrapped_root(cx);
    cx.simulate_window_resize(win.into(), size(px(width), px(height)));
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Performance;
        *v.system_snapshot_mut_for_test() = contract_page_snapshot();
        v.selected = device;
        cx.notify();
    });
    draw(cx, win);
    VisualTestContext::from_window(win.into(), cx)
}

/// The statistics rail holds its readable floor and never paints outside the
/// window, at every device-page × width combination of the matrix. Hidden is
/// the only sanctioned absence; a painted rail below the floor or past the
/// right edge is a squeezed/clipped sliver (the regression this guard pins).
#[gpui::test]
async fn stats_rail_never_squeezes_or_clips_across_the_width_matrix(cx: &mut TestAppContext) {
    for device in [
        SelectedDevice::Cpu,
        SelectedDevice::Memory,
        SelectedDevice::Nic(0),
    ] {
        for (width, height) in [
            (720.0_f32, 760.0_f32),
            (860.0, 750.0),
            (960.0, 750.0),
            (1024.0, 750.0),
            (1180.0, 780.0),
            (1920.0, 1080.0),
        ] {
            let mut vcx = draw_perf_page_at(cx, device, width, height);
            let viewport = vcx
                .debug_bounds(PERF_MAIN_VIEWPORT_SELECTOR)
                .unwrap_or_else(|| panic!("{device:?}@{width}: main viewport must compose"));
            assert!(
                viewport.size.width >= px(280.0) && viewport.size.height >= px(180.0),
                "{device:?}@{width}: main viewport collapsed: {viewport:?}"
            );
            if let Some(stats) = vcx.debug_bounds("tm-perf-stats-surface") {
                assert!(
                    stats.size.width >= px(PERFORMANCE_STATS_MIN_WIDTH),
                    "{device:?}@{width}: stats rail squeezed below its floor: {stats:?}"
                );
                assert!(
                    stats.origin.x + stats.size.width <= px(width),
                    "{device:?}@{width}: stats rail paints past the window edge (clipped sliver): {stats:?}"
                );
            }
            drop(vcx);
        }
    }
}

/// Below the app's own minimum width (a tiling compositor may force any
/// size), the page still degrades through the typed ladder: the chrome and
/// headline survive and the rail never paints squeezed. This is the
/// "bottom line of elasticity": arbitrary smallness yields Hidden/Stacked,
/// never overflow.
#[gpui::test]
async fn sub_floor_width_degrades_through_the_ladder_without_overflow(cx: &mut TestAppContext) {
    for (width, height) in [(320.0_f32, 480.0_f32), (540.0, 480.0), (640.0, 750.0)] {
        let mut vcx = draw_perf_page_at(cx, SelectedDevice::Nic(0), width, height);
        let title = vcx
            .debug_bounds("tm-perf-title")
            .unwrap_or_else(|| panic!("{width}: title must survive sub-floor widths"));
        assert!(
            title.size.width > px(40.0),
            "{width}: title collapsed at sub-floor width: {title:?}"
        );
        if let Some(stats) = vcx.debug_bounds("tm-perf-stats-surface") {
            assert!(
                stats.size.width >= px(PERFORMANCE_STATS_MIN_WIDTH)
                    || stats.size.width >= px(width),
                "{width}: sub-floor stats rail is neither floored nor full-width: {stats:?}"
            );
            assert!(
                stats.origin.x + stats.size.width <= px(width),
                "{width}: sub-floor stats rail clips past the window edge: {stats:?}"
            );
        }
        drop(vcx);
    }
}

/// The network page's vital line paints readable at every width. Geometry
/// only — the truncation-poisoning regression this complements is pinned by
/// the niri screenshot matrix (see the module doc).
#[gpui::test]
async fn network_vital_line_paints_readable_at_every_width(cx: &mut TestAppContext) {
    for (width, height) in [(320.0_f32, 480.0_f32), (720.0, 760.0), (1920.0, 1080.0)] {
        let mut vcx = draw_perf_page_at(cx, SelectedDevice::Nic(0), width, height);
        let vital = vcx
            .debug_bounds("tm-perf-vital-line")
            .unwrap_or_else(|| panic!("{width}: network vital line must render"));
        assert!(
            vital.size.width > px(40.0) && vital.size.height > px(10.0),
            "{width}: vital line collapsed: {vital:?}"
        );
        drop(vcx);
    }
}

fn seed_16_core_cpu(cx: &mut TestAppContext, width: f32, height: f32) -> VisualTestContext {
    let (win, view) = wrapped_root(cx);
    cx.simulate_window_resize(win.into(), size(px(width), px(height)));
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Performance;
        v.selected = SelectedDevice::Cpu;
        v.system_snapshot_mut_for_test().cpu =
            CpuMetrics::from_observations(CpuScalarObservations {
                global_usage_pct: ScalarObservation::available(42.0, 10),
                core_usage_group: ScalarObservationGroup::available(
                    (0..16).map(|i| i as f32).collect(),
                    10,
                ),
                ..Default::default()
            });
        cx.notify();
    });
    draw(cx, win);
    VisualTestContext::from_window(win.into(), cx)
}

/// CPU vertical policy: the per-core matrix composes only when every row can
/// keep its 40px floor beneath the companion headline floor; otherwise the
/// matrix hides WHOLE and the uncapped headline fills the viewport. When the
/// matrix is visible the headline is capped — small by default, the matrix
/// is the page's density. 1280x720 is a NORMAL window: the matrix must be
/// visible there.
#[gpui::test]
async fn cpu_matrix_hides_whole_below_row_floors_and_headline_fills(cx: &mut TestAppContext) {
    // Short window: the summed floors (chrome + headline floor + matrix
    // rows) exceed the viewport, so the matrix hides and the headline takes
    // everything.
    {
        let mut vcx = seed_16_core_cpu(cx, 1180.0, 600.0);
        assert!(
            vcx.debug_bounds("tm-cpu-per-core-matrix").is_none(),
            "matrix must hide whole when its row floors cannot fit"
        );
        let headline = vcx
            .debug_bounds("tm-perf-chart-card:cpu-headline-graph")
            .expect("headline survives the hide");
        assert!(
            headline.size.height >= px(280.0),
            "the uncapped headline fills the viewport when the matrix hides: {headline:?}"
        );
        drop(vcx);
    }
    // Tall window: every row keeps its floor and the headline stays small.
    {
        let mut vcx = seed_16_core_cpu(cx, 1180.0, 960.0);
        assert!(
            vcx.debug_bounds("tm-cpu-per-core-matrix").is_some(),
            "the matrix composes when the summed floors fit"
        );
        for selector in ["tm-perf-core:0", "tm-perf-core:15"] {
            let cell = vcx
                .debug_bounds(selector)
                .unwrap_or_else(|| panic!("core cell {selector} must render"));
            assert!(
                cell.size.height >= px(44.0),
                "{selector} squeezed below its floor: {cell:?}"
            );
        }
        let headline = vcx
            .debug_bounds("tm-perf-chart-card:cpu-headline-graph")
            .expect("headline beside the matrix");
        assert!(
            headline.size.height <= px(250.0),
            "the headline stays small while the matrix carries the page: {headline:?}"
        );
        drop(vcx);
    }
}

/// Memory vertical policy (the page's own ladder): the two headline charts
/// keep their floors for as long as possible; the composition detail
/// degrades TEXT-first (Full → Compact caption+bar → gone at the Floor
/// rung). A swap chart that cannot meet its floor hides instead of
/// overflowing the fixed viewport.
#[gpui::test]
async fn memory_detail_degrades_text_first_and_keeps_chart_floors(cx: &mut TestAppContext) {
    // Generous height: the full multi-line detail and both charts.
    {
        let mut vcx = draw_perf_page_at(cx, SelectedDevice::Memory, 1180.0, 1200.0);
        assert!(
            vcx.debug_bounds("tm-mem-detail-full").is_some(),
            "the full multi-line detail composes at a generous height"
        );
        assert!(vcx.debug_bounds("tm-perf-chart-card:swap-graph").is_some());
        drop(vcx);
    }
    // Reference height: text omitted first — the detail compacts to the
    // composition bar while BOTH charts keep their floors.
    {
        let mut vcx = draw_perf_page_at(cx, SelectedDevice::Memory, 1180.0, 780.0);
        assert!(
            vcx.debug_bounds("tm-mem-detail-compact").is_some(),
            "the detail must degrade text-first at the reference height"
        );
        assert!(vcx.debug_bounds("tm-mem-detail-full").is_none());
        for selector in [
            "tm-perf-chart-card:mem-graph",
            "tm-perf-chart-card:swap-graph",
        ] {
            let chart = vcx
                .debug_bounds(selector)
                .unwrap_or_else(|| panic!("{selector} must keep its floor"));
            assert!(
                chart.size.height >= px(180.0),
                "{selector} squeezed below its floor: {chart:?}"
            );
        }
        drop(vcx);
    }
    // Narrow column: the prose form is width-gated off (wrapping grows the
    // card past any height budget), so the compact detail coexists with the
    // aggregate chart. The swap chart stays governed by the width-axis
    // inventory gate (UltraCompact keeps the aggregate only).
    {
        let mut vcx = draw_perf_page_at(cx, SelectedDevice::Memory, 720.0, 1000.0);
        assert!(vcx.debug_bounds("tm-mem-detail-compact").is_some());
        let chart = vcx
            .debug_bounds("tm-perf-chart-card:mem-graph")
            .expect("aggregate chart must keep its floor at narrow width");
        assert!(chart.size.height >= px(180.0), "{chart:?}");
        assert!(vcx.debug_bounds("tm-perf-chart-card:swap-graph").is_none());
        drop(vcx);
    }
    // Short height: the swap chart hides whole; the memory chart keeps its
    // floor beside the compact detail.
    {
        let mut vcx = draw_perf_page_at(cx, SelectedDevice::Memory, 1180.0, 480.0);
        assert!(
            vcx.debug_bounds("tm-perf-chart-card:swap-graph").is_none(),
            "the swap chart hides whole when its floor cannot fit"
        );
        let mem = vcx
            .debug_bounds("tm-perf-chart-card:mem-graph")
            .expect("memory chart survives");
        assert!(mem.size.height >= px(180.0));
        drop(vcx);
    }
}
