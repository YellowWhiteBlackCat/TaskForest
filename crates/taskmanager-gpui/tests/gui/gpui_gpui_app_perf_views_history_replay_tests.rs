use super::*;
use crate::gpui_app::root::{RootView, TopPage};
use gpui::{AppContext, TestAppContext};
use taskmanager_core::core::DeviceId;
use taskmanager_core::core::HistoryMetric;
use taskmanager_theme::Theme;

#[test]
fn row_headings_carry_the_series_scope() {
    assert_eq!(
        row_heading(&HistorySeriesKey::system(HistoryMetric::CpuUsagePct)),
        "cpu-usage-pct"
    );
    assert_eq!(
        row_heading(&HistorySeriesKey::for_core(
            HistoryMetric::CpuCoreUsagePct,
            3
        )),
        "cpu-core-usage-pct · core 3"
    );
    assert!(
        row_heading(&HistorySeriesKey::for_device(
            HistoryMetric::GpuUsagePct,
            DeviceId::new("card0")
        ))
        .ends_with("card0")
    );
}

/// Persistence-disabled roots cannot open or render replay content.
#[gpui::test]
async fn replay_without_a_query_never_takes_over_the_live_graphs(cx: &mut TestAppContext) {
    let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    let view = win.entity(cx).expect("window root RootView entity");
    view.update(cx, |view, cx| {
        view.page = TopPage::Performance;
        view.mark_telemetry_frame_ready();
        view.toggle_history_replay(cx);
        // Persistence disabled: a missing client rejects the open transition.
        assert!(!view.history_replay_state().is_open());
        assert!(!view.history_replay_entry_available());
        assert!(!view.history_replay_visible());
        cx.notify();
    });
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
}

#[gpui::test]
async fn real_replay_keeps_toolbar_and_viewport_bounds_and_reaches_last_curve(
    cx: &mut TestAppContext,
) {
    use gpui::{Keystroke, VisualTestContext, px, size};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
    use taskmanager_app_host::NativeAppHost;
    use taskmanager_core::core::history::ApplicationHistoryIdentity;
    use taskmanager_core::core::time::LocalTimeRulesObservation;
    let root = crate::test_support::scratch_dir("history-review-layout");
    let history = root.join("history");
    std::fs::create_dir_all(&history).expect("private history directory");
    let now = u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_millis(),
    )
    .expect("stamp");
    for (stem, value) in [
        ("cpu-usage-pct", 31),
        ("gpu-usage-pct", 42),
        ("memory-used-pct", 57),
        ("network-rate-bps", 9000),
        ("swap-used-pct", 7),
    ] {
        let data = format!(
            "{{\"r\":1,\"c\":{},\"m\":1,\"v\":{}}}\n{{\"r\":2,\"c\":{},\"m\":2,\"v\":{}}}\n",
            now - 1000,
            value / 2,
            now,
            value
        );
        std::fs::write(history.join(format!("{stem}__-__-.jsonl")), data)
            .expect("typed query input");
    }
    let app_key = HistorySeriesKey::for_application(
        HistoryMetric::ApplicationCpuUsagePct,
        ApplicationHistoryIdentity::verified_launcher("org.example.App").expect("identity"),
    );
    std::fs::write(
        history.join(format!("{}.jsonl", app_key.file_stem())),
        format!("{{\"r\":1,\"c\":{now},\"m\":{now},\"v\":31}}\n"),
    )
    .expect("typed application history input");
    let host = NativeAppHost::from_paths(
        root.join("config.json"),
        history,
        LocalTimeRulesObservation::unsupported(0),
    );
    let window = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    let view = window.entity(cx).expect("root");
    view.update(cx, |view, cx| {
        view.page = TopPage::Performance;
        view.mark_telemetry_frame_ready();
        view.history_runtime.request(true);
        view.history_runtime
            .install_connector(host.history_frontend_connector());
        cx.notify();
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let ready = view.update(cx, |view, cx| {
            view.drain_history_replay_completions(cx);
            view.history_replay_state().rows().len() == 5
        });
        if ready {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "real query must publish all five series"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    view.update(cx, |view, cx| view.toggle_history_replay(cx));
    for (width, height) in [
        (480.0, 360.0),
        (720.0, 480.0),
        (1180.0, 780.0),
        (1600.0, 420.0),
        (720.0, 960.0),
    ] {
        cx.simulate_window_resize(window.into(), size(px(width), px(height)));
        view.update(cx, |view, _cx| {
            view.page = TopPage::Performance;
            view.history_replay_scroll
                .set_offset(gpui::point(px(0.0), px(0.0)))
        });
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
            .expect("draw");
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        let controls = visual
            .debug_bounds("tm-replay-controls")
            .expect("fixed controls mount");
        let viewport = visual
            .debug_bounds("tm-replay-viewport")
            .expect("bounded review viewport mounts");
        let title = visual.debug_bounds("tm-replay-title").expect("title slot");
        let subtitle = visual
            .debug_bounds("tm-replay-subtitle")
            .expect("context slot");
        for bounds in [controls, viewport, title, subtitle] {
            assert!(bounds.size.height > px(0.0));
            assert!(
                bounds.left() >= px(0.0) && bounds.right() <= px(width),
                "right edge is protected at {width}x{height}: {bounds:?}"
            );
            assert!(
                bounds.top() >= px(0.0) && bounds.bottom() <= px(height),
                "bottom edge is protected: {bounds:?}"
            );
        }
        assert!(
            controls.bottom() <= viewport.top(),
            "the toolbar precedes the body vertically"
        );
        window
            .update(cx, |_view, window, _cx| window.focus_next())
            .expect("focus route");
        cx.dispatch_keystroke(window.into(), Keystroke::parse("end").expect("end"));
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
            .expect("scroll draw");
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        assert_eq!(
            visual.debug_bounds("tm-replay-controls"),
            Some(controls),
            "scrolling never moves the actions"
        );
        assert_eq!(
            visual.debug_bounds("tm-replay-viewport"),
            Some(viewport),
            "scrolling never moves the viewport"
        );
        let last = visual
            .debug_bounds("tm-replay-curve-4")
            .expect("last queried curve is reachable");
        assert!(
            last.top() >= viewport.top() && last.bottom() <= viewport.bottom(),
            "the full last curve fits after normal End navigation: {last:?} in {viewport:?}"
        );
        view.update(cx, |view, _cx| view.page = TopPage::AppHistory);
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
            .expect("application history draw");
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        for selector in [
            "tm-app-history-page-header",
            "tm-app-history-controls",
            "tm-app-history-list",
        ] {
            let bounds = visual
                .debug_bounds(selector)
                .expect("real application history mounts");
            assert!(bounds.size.height > px(0.0));
            assert!(bounds.left() >= px(0.0) && bounds.right() <= px(width));
            assert!(bounds.top() >= px(0.0) && bounds.bottom() <= px(height));
        }
    }
    drop(host);
}
