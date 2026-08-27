//! CPU page fixed-composition and geometry behavior.

use super::*;

fn cpu_with_all_metrics() -> CpuMetrics {
    CpuMetrics::from_observations(CpuScalarObservations {
        global_usage_pct: ScalarObservation::available(42.0, 10),
        core_usage_group: ScalarObservationGroup::available(vec![12.0, 55.0, 80.0, 30.0], 10),
        frequency_mhz: ScalarObservation::available(3_200, 10),
        max_frequency_mhz: ScalarObservation::available(4_800, 10),
        per_core_frequency_group: ScalarObservationGroup::available(
            vec![3_200, 3_100, 2_800, 2_600],
            10,
        ),
        temperature_c: ScalarObservation::available(47.0, 10),
        per_core_temperature_group: ScalarObservationGroup::available(
            vec![46.0, 47.0, 48.0, 45.0],
            10,
        ),
        power_w: ScalarObservation::available(18.5, 10),
    })
}

/// The CPU view paints one honest fallback cell before indexed data and one
/// cell per reported core after the provider publishes the vector.
#[gpui::test]
async fn cpu_page_paints_one_cell_per_reported_core(cx: &mut TestAppContext) {
    let (win, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Performance;
        v.selected = SelectedDevice::Cpu;
        cx.notify();
    });
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    assert!(vcx.debug_bounds("tm-perf-core:1").is_none());
    assert!(vcx.debug_bounds("tm-perf-core:0").is_some());

    view.update(cx, |v, cx| {
        v.system_snapshot_mut_for_test().cpu =
            CpuMetrics::from_observations(CpuScalarObservations {
                core_usage_group: ScalarObservationGroup::available(
                    vec![12.0, 55.0, 80.0, 30.0],
                    10,
                ),
                frequency_mhz: ScalarObservation::available(3_200, 10),
                temperature_c: ScalarObservation::available(47.0, 10),
                ..Default::default()
            });
        cx.notify();
    });
    vcx.update(|window, cx| window.draw(cx).clear());
    for (index, selector) in [
        (0, "tm-perf-core:0"),
        (1, "tm-perf-core:1"),
        (2, "tm-perf-core:2"),
        (3, "tm-perf-core:3"),
    ] {
        let bounds = vcx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("core cell {index} must render"));
        assert!(bounds.size.height > px(20.0));
        assert!(bounds.size.width > px(0.0));
    }
    let first = vcx.debug_bounds("tm-perf-core:0").expect("cell 0");
    let third = vcx.debug_bounds("tm-perf-core:2").expect("cell 2");
    assert!(third.origin.y > first.origin.y);
}

/// The dominant aggregate graph, simultaneous readouts, per-core matrix, and
/// details remain reachable without renderer selection state.
#[gpui::test]
async fn cpu_page_renders_dominant_graph_readouts_and_per_core_content(cx: &mut TestAppContext) {
    let (win, view) = wrapped_root(cx);
    cx.simulate_window_resize(win.into(), size(px(1180.0), px(780.0)));
    view.update(cx, |view, cx| {
        view.mark_telemetry_frame_ready();
        view.page = TopPage::Performance;
        view.selected = SelectedDevice::Cpu;
        view.system_snapshot_mut_for_test().cpu = cpu_with_all_metrics();
        cx.notify();
    });
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let main_surface = vcx
        .debug_bounds("tm-perf-main-surface")
        .expect("shared Performance main surface");
    let stats_surface = vcx
        .debug_bounds("tm-perf-stats-surface")
        .expect("shared pinned stats surface");
    let utilization = vcx
        .debug_bounds("tm-cpu-main-utilization-graph")
        .expect("dominant utilization graph");
    assert!(utilization.left() >= main_surface.left() - px(0.5));
    assert!(
        utilization.right() <= stats_surface.left() - px(15.0),
        "CPU graph content must keep its internal trailing inset before the pinned rail: graph={utilization:?}, stats={stats_surface:?}"
    );
    assert_eq!(
        main_surface.right(),
        stats_surface.left(),
        "the rail surface still owns the outer split seam"
    );
    let heading_leading = vcx
        .debug_bounds("tm-perf-title-text")
        .expect("CPU heading leading slot");
    let heading_trailing = vcx
        .debug_bounds("tm-perf-subtitle-text")
        .expect("CPU heading trailing slot");
    assert!(heading_trailing.left() > heading_leading.left());
    assert!(
        heading_trailing.right() >= utilization.right() - px(1.0),
        "the CPU model slot must consume the remaining heading width: leading={heading_leading:?}, trailing={heading_trailing:?}, graph={utilization:?}"
    );
    assert!(vcx.debug_bounds("tm-cpu-readouts").is_some());
    let first_core = vcx
        .debug_bounds("tm-perf-core:0")
        .expect("first per-core graph");
    let chart_surface = main_surface;
    assert!(
        utilization.size.height > first_core.size.height,
        "the aggregate graph ({:?}) must remain taller than a per-core graph ({:?})",
        utilization.size.height,
        first_core.size.height
    );
    let last_core = vcx
        .debug_bounds("tm-perf-core:3")
        .expect("last per-core graph");
    assert!(last_core.bottom() <= chart_surface.bottom() + px(0.5));
    assert!(stats_surface.size.height >= main_surface.size.height - px(1.0));
    let panel = vcx
        .debug_bounds("tm-cpu-details-panel")
        .expect("pinned CPU details");
    assert!(panel.right() <= stats_surface.right() + px(0.5));

    let (compact_win, compact_view) = wrapped_root(cx);
    cx.simulate_window_resize(compact_win.into(), size(px(720.0), px(480.0)));
    compact_view.update(cx, |view, cx| {
        view.mark_telemetry_frame_ready();
        view.page = TopPage::Performance;
        view.selected = SelectedDevice::Cpu;
        view.system_snapshot_mut_for_test().cpu = cpu_with_all_metrics();
        cx.notify();
    });
    draw(cx, compact_win);
    let mut vcx = VisualTestContext::from_window(compact_win.into(), cx);
    let compact_surface = vcx
        .debug_bounds("tm-cpu-chart-surface")
        .expect("compact elastic CPU chart surface");
    let graph = vcx
        .debug_bounds("tm-cpu-main-utilization-graph")
        .expect("compact dominant utilization graph");
    assert!(graph.left() >= compact_surface.left() - px(0.5));
    assert!(
        graph.right() <= compact_surface.right() - px(7.0),
        "rail-less ultra-compact CPU view must retain a real trailing page inset"
    );
    assert!(graph.size.height > px(180.0));
    assert!(vcx.debug_bounds("tm-cpu-readouts").is_some());
    assert!(vcx.debug_bounds("tm-cpu-per-core-matrix").is_none());
}

/// Width and height are independent layout axes: a panoramic, short window
/// keeps the horizontal sidebar/details composition but collapses the
/// height-hungry per-core matrix.
#[gpui::test]
async fn cpu_page_uses_wide_short_budget_without_reintroducing_compact_mode(
    cx: &mut TestAppContext,
) {
    let (win, view) = wrapped_root(cx);
    cx.simulate_window_resize(win.into(), size(px(2048.0), px(540.0)));
    view.update(cx, |view, cx| {
        view.mark_telemetry_frame_ready();
        view.page = TopPage::Performance;
        view.selected = SelectedDevice::Cpu;
        view.system_snapshot_mut_for_test().cpu = cpu_with_all_metrics();
        cx.notify();
    });
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let main = vcx
        .debug_bounds("tm-perf-main-surface")
        .expect("wide-short layout retains the main split surface");
    let stats = vcx
        .debug_bounds("tm-perf-stats-surface")
        .expect("wide-short layout retains the pinned details surface");
    let graph = vcx
        .debug_bounds("tm-cpu-main-utilization-graph")
        .expect("wide-short layout retains the dominant graph");
    assert!(vcx.debug_bounds("tm-cpu-per-core-matrix").is_none());
    assert_eq!(main.right(), stats.left());
    assert!(graph.right() <= stats.left() - px(15.0));
    assert!(graph.size.height > px(180.0));
}
