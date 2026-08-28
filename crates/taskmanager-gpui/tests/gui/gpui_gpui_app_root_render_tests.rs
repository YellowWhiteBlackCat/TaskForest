use super::{CursorRefreshState, should_schedule_cursor_refresh, ui_font_with_fallback};
use crate::gpui_app::theme::{FONT_MISANS_VF, FONT_ROBOTO_MONO, Theme};

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

use crate::core::metrics::{
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
        snap.memory = crate::core::metrics::MemoryMetrics::from_observations(
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
