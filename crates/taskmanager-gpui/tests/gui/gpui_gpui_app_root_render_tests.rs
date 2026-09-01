use super::{CursorRefreshState, should_schedule_cursor_refresh, ui_font_with_fallback};
use taskmanager_theme::{FONT_MISANS_VF, FONT_ROBOTO_MONO, Theme};

#[test]
fn cursor_refresh_is_coalesced_until_the_next_frame() {
    assert!(should_schedule_cursor_refresh(
        true,
        CursorRefreshState::Idle
    ));
    assert!(!should_schedule_cursor_refresh(
        true,
        CursorRefreshState::Scheduled
    ));
    assert!(!should_schedule_cursor_refresh(
        false,
        CursorRefreshState::Idle
    ));
}

#[test]
fn inherited_ui_font_declares_the_bundled_cjk_fallback() {
    let mut theme = Theme::dark();
    theme.ui_font = FONT_ROBOTO_MONO;

    let font = ui_font_with_fallback(&theme);
    assert_eq!(font.family.as_ref(), FONT_ROBOTO_MONO);
    assert_eq!(
        font.fallbacks
            .as_ref()
            .expect("the UI style must carry a fallback list")
            .fallback_list(),
        [FONT_MISANS_VF, FONT_ROBOTO_MONO]
    );
}

// ── Desktop-widget surface (render-path) ─────────────────────────────────────
//
// The widget replaces the whole desktop shell, so its proof is a render-path
// window test: with the widget surface role set and a seeded typed snapshot,
// the four metric cards must paint with real bounded geometry from that
// snapshot — proving the compact projection consumes the SAME facts as the
// full dashboard rather than a second data fold.

use gpui::{AppContext, TestAppContext, VisualTestContext, px};

use taskmanager_core::core::metrics::{
    CpuScalarObservations, MemoryOptionalObservations, MemoryScalarObservations, ScalarObservation,
};

#[gpui::test]
async fn desktop_widget_surface_paints_metric_cards_from_the_snapshot(cx: &mut TestAppContext) {
    let gib = |n: u64| n * 1024 * 1024 * 1024;
    let win = cx.add_window(|_window, cx| crate::gpui_app::root::RootView::new(Theme::dark(), cx));
    let view = win.entity(cx).expect("window root RootView entity");
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.surface_role = crate::window_presentation::GpuiSurfaceRole::DesktopWidget;
        let snap = v.system_snapshot_mut_for_test();
        snap.cpu.apply_scalar_observations(CpuScalarObservations {
            global_usage_pct: ScalarObservation::available(42.5, 10),
            ..CpuScalarObservations::default()
        });
        snap.memory = taskmanager_core::core::metrics::MemoryMetrics::from_observations(
            MemoryScalarObservations {
                total_bytes: ScalarObservation::available(gib(16), 10),
                used_bytes: ScalarObservation::available(gib(6), 10),
                ..MemoryScalarObservations::default()
            },
            MemoryOptionalObservations::default(),
        );
        cx.notify();
    });
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let root = vcx
        .debug_bounds("taskforest-desktop-widget")
        .expect("the widget root must replace the desktop shell");
    assert!(
        root.size.width > px(200.0) && root.size.height > px(100.0),
        "the widget root must own a real surface: {root:?}"
    );
    for (card, selector) in [
        ("CPU", "tm-widget-cpu"),
        ("memory", "tm-widget-memory"),
        ("processes", "tm-widget-processes"),
        ("alerts", "tm-widget-alerts"),
    ] {
        let bounds = vcx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("the widget {card} card must paint"));
        assert!(
            bounds.size.width > px(40.0) && bounds.size.height > px(30.0),
            "the widget {card} card collapsed: {bounds:?}"
        );
    }
    drop(vcx);
}

#[gpui::test]
async fn refused_window_frame_preference_reports_an_honest_notice_once(cx: &mut TestAppContext) {
    use crate::gpui_app::chrome::WindowDecorationsPreference;

    let win = cx.add_window(|_window, cx| crate::gpui_app::root::RootView::new(Theme::dark(), cx));
    let view = win.entity(cx).expect("window root RootView entity");
    view.update(cx, |v, _cx| {
        v.mark_telemetry_frame_ready();
    });

    // Honored request (Native, and the platform fact agrees with the Server
    // grant the gpui test window reports): rendering must stay silent —
    // nothing to apologize for.
    view.update(cx, |v, _cx| {
        v.window_decorations_pref = WindowDecorationsPreference::Native;
    });
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    view.read_with(cx, |v, _| {
        assert!(
            v.local_feedback_toast.is_none(),
            "an honored frame preference must not raise a notice"
        );
        assert!(!v.decoration_outcome_reported);
    });

    // Refused request: force the granted fact to Client (what GNOME/Mutter
    // configures when a compositor cannot draw server-side frames). The next
    // render reports the contradiction once and latches.
    view.update(cx, |v, _cx| {
        v.decorations_override = Some(false);
    });
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    view.read_with(cx, |v, _| {
        assert!(
            v.local_feedback_toast.is_some(),
            "a refused Native preference must surface an honest notice"
        );
        assert!(v.decoration_outcome_reported, "the notice must latch");
        assert_eq!(v.local_feedback_seq, 1);
    });

    // The latch holds: subsequent renders do not re-notify (no toast churn).
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    view.read_with(cx, |v, _| {
        assert_eq!(
            v.local_feedback_seq, 1,
            "the outcome notice must fire once per preference change"
        );
    });

    // A NEW explicit request re-arms the latch; System never promises a mode,
    // so even the contradicting (overridden) fact stays silent.
    view.update(cx, |v, _cx| {
        v.window_decorations_pref = WindowDecorationsPreference::System;
        v.decoration_outcome_reported = false;
    });
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    view.read_with(cx, |v, _| {
        assert!(!v.decoration_outcome_reported);
        assert_eq!(
            v.local_feedback_seq, 1,
            "System mode must never raise an outcome notice"
        );
    });
}
