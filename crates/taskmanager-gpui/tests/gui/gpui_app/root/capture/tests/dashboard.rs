//! Dashboard capture-evidence tests.
//!
//! These exercise the dashboard-side readiness gate of `CaptureEvidence`
//! (`on_dashboard_state`, panel routing, history-window anchoring) — the
//! process/snapshot/scenario-token cases live in the parent module.

use super::super::{
    CaptureEvidence, CaptureScenario, DashboardPanel, DashboardState, SystemSnapshot,
};
use super::PROCESSES_OBSERVED_AT_MS;
use crate::gpui_app::root::RootView;
use crate::gpui_app::root::TopPage;
use gpui::{AppContext, TestAppContext, VisualTestContext, px, size};
use std::sync::Arc;
use taskmanager_application::system_timeline::{SystemHistoryWindow, SystemPageSection};
use taskmanager_application::system_timeline::{
    TimelineMetric, TimelineSelection, TimelineStatistic,
};
use taskmanager_assets::embedded_fonts;
use taskmanager_shell::fixture::dashboard_history::seed_system_dashboard_history;
use taskmanager_telemetry_store::HistoryRetention;
use taskmanager_telemetry_store::{CorrelatedSystemTelemetryIngestor, TelemetryStore};
use taskmanager_test_support::pin_english;
use taskmanager_theme::Theme;

#[gpui::test]
async fn dashboard_cards_stay_inside_the_bounded_review_viewport(cx: &mut TestAppContext) {
    use crate::gpui_app::chrome::titlebar_height;
    use crate::gpui_app::root::responsive::{DashboardBudget, FrameBudget, FrameChromeBudget};
    use std::collections::HashSet;
    pin_english();
    cx.update(|cx| {
        cx.text_system()
            .add_fonts(embedded_fonts())
            .expect("embedded fonts")
    });
    for (width, height) in [
        (480.0, 360.0),
        (720.0, 480.0),
        (1280.0, 720.0),
        (1600.0, 360.0),
        (480.0, 960.0),
        (1920.0, 1080.0),
    ] {
        let win = cx.add_window(|_, cx| RootView::new_demo(Theme::dark(), cx));
        cx.simulate_window_resize(win.into(), size(px(width), px(height)));
        win.update(cx, |view, _, cx| {
            view.page = TopPage::System;
            view.decorations_override = Some(true);
            view.dashboard.section = SystemPageSection::Dashboard;
            view.dashboard.history_window = SystemHistoryWindow::SixtyMinutes;
            assert!(seed_system_dashboard_history(
                &view.telemetry.system_history,
                &view.telemetry_ingestor,
                7_200_000
            ));
            cx.notify();
        })
        .unwrap();
        let layout = win
            .read_with(cx, |view, _| {
                DashboardBudget::from_frame(FrameBudget::for_root(
                    size(px(width), px(height)),
                    view.nav_orientation,
                    FrameChromeBudget::new(titlebar_height(&view.theme), false, false),
                ))
            })
            .unwrap();
        let mut reached = HashSet::new();
        for _ in 0..4 {
            cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
                .unwrap();
            let first = win
                .read_with(cx, |view, _| {
                    layout.first_metric(view.dashboard.history_first_metric)
                })
                .unwrap();
            let mut visual = VisualTestContext::from_window(win.into(), cx);
            let viewport = visual
                .debug_bounds("tm-dashboard-review")
                .expect("bounded review slot");
            let controls = visual
                .debug_bounds("tm-dashboard-window-controls")
                .expect("fixed window controls");
            let paging = visual
                .debug_bounds("tm-dashboard-paging")
                .expect("fixed paging");
            assert!(viewport.size.width > px(0.0) && viewport.size.height > px(0.0));
            assert!(viewport.right() <= px(width) && viewport.bottom() <= px(height));
            assert!(controls.bottom() <= paging.top() && paging.bottom() <= viewport.top());
            if layout.metric_count == 0 {
                assert!(visual.debug_bounds("tm-dashboard-resize").is_some());
                break;
            }
            for (metric, selector) in [
                (TimelineMetric::Cpu, "tm-dashboard-history-card:cpu"),
                (TimelineMetric::Memory, "tm-dashboard-history-card:memory"),
                (TimelineMetric::Disk, "tm-dashboard-history-card:disk"),
                (TimelineMetric::Network, "tm-dashboard-history-card:network"),
            ]
            .into_iter()
            .skip(first)
            .take(layout.metric_count)
            {
                let card = visual
                    .debug_bounds(selector)
                    .expect("admitted complete card");
                assert!(card.size.height >= px(112.0), "{width}x{height}: {card:?}");
                assert!(card.left() >= viewport.left() && card.right() <= viewport.right());
                assert!(
                    card.top() >= viewport.top() && card.bottom() + px(7.0) <= viewport.bottom(),
                    "whole group and bottom safety: {width}x{height}: {card:?} in {viewport:?}"
                );
                reached.insert(metric);
            }
            let next = visual
                .debug_bounds("dashboard-next-metrics")
                .expect("normal next button");
            visual.simulate_click(next.center(), Default::default());
        }
        if layout.metric_count != 0 {
            assert_eq!(
                reached.len(),
                TimelineMetric::ALL.len(),
                "all metrics reachable: {width}x{height}"
            );
        }
        for window in SystemHistoryWindow::ALL {
            cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
                .unwrap();
            let mut visual = VisualTestContext::from_window(win.into(), cx);
            let control = visual
                .debug_bounds(window.id())
                .expect("normal window button");
            visual.simulate_click(control.center(), Default::default());
            drop(visual);
            win.read_with(cx, |view, _| {
                assert_eq!(view.dashboard.history_window, window);
                assert_eq!(view.dashboard.history_first_metric, 0);
                assert_eq!(
                    view.dashboard
                        .timeline
                        .series(&view.telemetry.system_history, window)
                        .covered_ms,
                    window.minutes() * 60_000
                );
            })
            .unwrap();
        }
    }
}

#[gpui::test]
fn dashboard_capture_isolates_host_history_and_keeps_one_authority(cx: &mut TestAppContext) {
    let window = cx.add_window(|_, cx| RootView::new(Theme::dark(), cx));
    window
        .update(cx, |view, _, _| {
            let host_store = Arc::clone(&view.telemetry);
            // Production and not-yet-ready captures must preserve the live store.
            view.prepare_dashboard_capture_history();
            assert!(Arc::ptr_eq(&host_store, &view.telemetry));
            view.capture_evidence =
                CaptureEvidence::for_test(Some(CaptureScenario::HistorySixtyMinutes));
            view.prepare_dashboard_capture_history();
            assert!(Arc::ptr_eq(&host_store, &view.telemetry));
            let mut snapshot = SystemSnapshot::default();
            let mut processes = Vec::new();
            view.capture_evidence.on_snapshot(&mut snapshot);
            view.capture_evidence.on_processes_update(
                true,
                PROCESSES_OBSERVED_AT_MS,
                &mut processes,
            );
            view.prepare_dashboard_capture_history();
            assert!(!Arc::ptr_eq(&host_store, &view.telemetry));
            view.capture_evidence.on_dashboard_state(
                &mut view.dashboard,
                &view.telemetry.system_history,
                &view.telemetry_ingestor,
                7_200_000,
            );
            let fixture_store = Arc::clone(&view.telemetry);
            view.prepare_dashboard_capture_history();
            assert!(Arc::ptr_eq(&fixture_store, &view.telemetry));
            let series = view.dashboard.timeline.series(
                &view.telemetry.system_history,
                SystemHistoryWindow::SixtyMinutes,
            );
            for (metric, latest, peak) in [
                (TimelineMetric::Cpu, 24.0, 99.0),
                (TimelineMetric::Memory, 48.0, 58.5),
                (TimelineMetric::Disk, 0.0, 44.0),
                (
                    TimelineMetric::Network,
                    0.0,
                    34_200_000.0 / (1024.0 * 1024.0),
                ),
            ] {
                for (statistic, expected) in [
                    (TimelineStatistic::Latest, latest),
                    (TimelineStatistic::Peak, peak),
                ] {
                    let actual = series
                        .readout(TimelineSelection { metric, statistic })
                        .expect("accepted fixture observation")
                        .value;
                    assert!(
                        (actual - expected).abs() < 0.01,
                        "{metric:?} {statistic:?}: {actual}"
                    );
                }
            }
        })
        .unwrap();
}

fn history_pair() -> (Arc<TelemetryStore>, CorrelatedSystemTelemetryIngestor) {
    TelemetryStore::shared_with_correlated_ingestion(HistoryRetention::PRODUCT)
}

#[test]
fn dashboard_marker_waits_for_live_readiness_and_prepares_exact_target() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::EventCenter));
    let mut dashboard = DashboardState::new();
    let (store, ingestor) = history_pair();
    assert_eq!(
        evidence.on_dashboard_state(&mut dashboard, &store.system_history, &ingestor, 7_200_000,),
        None
    );
    assert!(!evidence.scenario_ready());
    let mut snapshot = SystemSnapshot::default();
    evidence.on_snapshot(&mut snapshot);
    assert_eq!(
        evidence.on_dashboard_state(&mut dashboard, &store.system_history, &ingestor, 7_200_000,),
        None
    );
    assert!(!evidence.scenario_ready());
    let mut processes = Vec::new();
    assert!(
        evidence
            .on_processes_update(true, PROCESSES_OBSERVED_AT_MS, &mut processes)
            .is_none()
    );
    let panel =
        evidence.on_dashboard_state(&mut dashboard, &store.system_history, &ingestor, 7_200_000);
    assert!(evidence.scenario_ready());
    assert_eq!(panel, Some(DashboardPanel::Events));
    let events = evidence
        .take_event_history_fixture()
        .expect("event-center capture supplies shared history");
    assert_eq!(dashboard.events.unread_count(&events), 2);
}

#[test]
fn every_dashboard_capture_token_reaches_its_exact_root_state() {
    fn prepared(
        scenario: CaptureScenario,
    ) -> (DashboardState, Arc<TelemetryStore>, Option<DashboardPanel>) {
        let mut evidence = CaptureEvidence::for_test(Some(scenario));
        let mut dashboard = DashboardState::new();
        let (store, ingestor) = history_pair();
        let mut snapshot = SystemSnapshot::default();
        let mut processes = Vec::new();
        evidence.on_snapshot(&mut snapshot);
        evidence.on_processes_update(true, PROCESSES_OBSERVED_AT_MS, &mut processes);
        let panel = evidence.on_dashboard_state(
            &mut dashboard,
            &store.system_history,
            &ingestor,
            7_200_000,
        );
        assert_eq!(
            evidence.scenario_ready(),
            scenario != CaptureScenario::SystemHardware,
            "hardware readiness requires its rendered inventory viewport"
        );
        (dashboard, store, panel)
    }
    let (overview, _, _) = prepared(CaptureScenario::SystemDashboard);
    assert_eq!(overview.section, SystemPageSection::Dashboard);
    assert_eq!(overview.history_window, SystemHistoryWindow::FifteenMinutes);
    let (hardware, _, _) = prepared(CaptureScenario::SystemHardware);
    assert_eq!(hardware.section, SystemPageSection::Hardware);
    let (history, store, _) = prepared(CaptureScenario::HistorySixtyMinutes);
    assert_eq!(history.history_window, SystemHistoryWindow::SixtyMinutes);
    assert_eq!(
        history
            .timeline
            .series(&store.system_history, SystemHistoryWindow::SixtyMinutes)
            .covered_ms,
        3_600_000
    );
    assert_eq!(
        prepared(CaptureScenario::AlertRulesManager).2,
        Some(DashboardPanel::AlertRules)
    );
    assert_eq!(
        prepared(CaptureScenario::EventCenter).2,
        Some(DashboardPanel::Events)
    );
    let (saved, _, panel) = prepared(CaptureScenario::SavedViewPresets);
    assert_eq!(panel, Some(DashboardPanel::SavedViews));
    assert!(saved.saved_views.iter().any(|preset| preset.id == 90_000));
}

#[test]
fn system_npu_capture_selects_hardware_but_defers_marker_to_post_layout_scroll() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::SystemNpu));
    let mut dashboard = DashboardState::new();
    let (store, ingestor) = history_pair();
    let mut snapshot = SystemSnapshot::default();
    let mut processes = Vec::new();
    evidence.on_snapshot(&mut snapshot);
    evidence.on_processes_update(true, PROCESSES_OBSERVED_AT_MS, &mut processes);

    let panel =
        evidence.on_dashboard_state(&mut dashboard, &store.system_history, &ingestor, 7_200_000);
    assert_eq!(dashboard.section, SystemPageSection::Hardware);
    assert_eq!(panel, None);
    assert!(
        !evidence.scenario_ready(),
        "the marker belongs to the later visible-scroll terminal"
    );
}

#[test]
fn dashboard_capture_history_stays_anchored_when_a_new_live_frame_arrives() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::HistorySixtyMinutes));
    let mut dashboard = DashboardState::new();
    let (store, ingestor) = history_pair();
    let mut snapshot = SystemSnapshot {
        timestamp_ms: 7_200_000,
        ..Default::default()
    };
    let mut processes = Vec::new();
    evidence.on_snapshot(&mut snapshot);
    evidence.on_processes_update(true, PROCESSES_OBSERVED_AT_MS, &mut processes);
    let _ = evidence.on_dashboard_state(
        &mut dashboard,
        &store.system_history,
        &ingestor,
        snapshot.timestamp_ms,
    );

    let _ =
        evidence.on_dashboard_state(&mut dashboard, &store.system_history, &ingestor, 7_201_000);

    assert_eq!(
        dashboard
            .timeline
            .series(&store.system_history, SystemHistoryWindow::SixtyMinutes)
            .covered_ms,
        3_600_000
    );
}
