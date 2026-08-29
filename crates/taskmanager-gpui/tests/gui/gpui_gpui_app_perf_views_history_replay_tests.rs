use super::*;
use crate::gpui_app::root::{RootView, TopPage};
use gpui::{AppContext, TestAppContext};
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
            taskmanager_core::core::DeviceId::new("card0")
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
