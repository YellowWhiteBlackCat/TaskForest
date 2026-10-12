use super::{CursorRefreshState, should_schedule_cursor_refresh, ui_font_with_fallback};
use gpui::{
    AppContext, Context, FocusHandle, InteractiveElement, IntoElement, ParentElement, Render,
    Styled, TestAppContext, VisualTestContext, Window, div,
};
use taskmanager_core::core::metrics::MemoryMetrics;
use taskmanager_theme::{FONT_MISANS_VF, FONT_ROBOTO_MONO, Theme};

struct FocusTargetHarness {
    focus_handle: Option<FocusHandle>,
}

impl Render for FocusTargetHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let root = div().size_full();
        if let Some(focus_handle) = &self.focus_handle {
            root.child(
                div()
                    .id("focus-lifecycle-target")
                    .size_full()
                    .track_focus(focus_handle),
            )
        } else {
            root
        }
    }
}

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

/// Focus-visible is a modality decision, not a focus decision: GPUI 0.2.2
/// attaches no input origin to `FocusHandle`, so the per-window state machine
/// is the sole source of the ring policy. The capture listeners call these
/// exact transitions before descendants handle the same event, and the render
/// snapshot consumes `shows_focus_ring()` through `Theme::with_focus_visible`.
#[test]
fn input_modality_only_keyboard_paints_the_focus_ring() {
    use super::super::InputModality;

    let mut modality = InputModality::default();
    assert!(
        !modality.shows_focus_ring(),
        "programmatic focus must stay calm (initial state)"
    );

    assert!(
        modality.observe_keyboard(),
        "the keyboard origin must be a real transition"
    );
    assert!(
        modality.shows_focus_ring(),
        "keyboard navigation must paint the ring"
    );
    assert!(
        !modality.observe_keyboard(),
        "a repeated key must not churn the render"
    );

    assert!(
        modality.observe_pointer(),
        "the pointer origin must be a real transition"
    );
    assert!(
        !modality.shows_focus_ring(),
        "pointer-driven focus must suppress the ring"
    );
    assert!(
        !modality.observe_pointer(),
        "a repeated press must not churn the render"
    );

    assert!(
        modality.observe_keyboard(),
        "the next keyboard origin must ring again"
    );
    assert!(modality.shows_focus_ring());
}

#[gpui::test]
async fn input_modality_clears_when_its_window_blurs(cx: &mut TestAppContext) {
    let window =
        cx.add_window(|_window, cx| crate::gpui_app::root::RootView::new_demo(Theme::dark(), cx));
    let root = window.entity(cx).expect("demo RootView entity");
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .expect("demo root should render");

    let mut visual = VisualTestContext::from_window(window.into(), cx);
    // TestWindow starts inactive, so activate it explicitly before asking the
    // visual harness to simulate a later window deactivation.
    visual.update(|window, _cx| window.activate_window());
    visual.run_until_parked();
    visual.simulate_keystrokes("tab");
    let keyboard_ring_alpha = root.read_with(&visual, |view, _| {
        view.theme
            .with_focus_visible(view.input_modality.shows_focus_ring())
            .palette()
            .ring
            .a
    });
    assert!(
        keyboard_ring_alpha > 0.0,
        "keyboard traversal must arm the focus ring"
    );

    visual.deactivate_window();
    visual.update(|window, cx| window.draw(cx).clear());
    let blurred_ring_alpha = root.read_with(&visual, |view, _| {
        view.theme
            .with_focus_visible(view.input_modality.shows_focus_ring())
            .palette()
            .ring
            .a
    });
    assert_eq!(
        blurred_ring_alpha, 0.0,
        "window deactivation must clear the keyboard focus-ring decision"
    );
}

#[gpui::test]
async fn input_modality_clears_when_focused_target_unmounts(cx: &mut TestAppContext) {
    let root = cx.new(|cx| crate::gpui_app::root::RootView::new_demo(Theme::dark(), cx));
    let window = cx.add_window(|_window, cx| FocusTargetHarness {
        focus_handle: Some(cx.focus_handle().tab_stop(true)),
    });
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .expect("focus-target harness should render");

    window
        .update(cx, |_view, window, cx| {
            root.update(cx, |root, cx| {
                assert!(root.input_modality.observe_keyboard());
                root.ensure_input_modality_observers(window, cx);
            });
        })
        .expect("focus lifecycle window should update");
    window
        .update(cx, |view, window, _cx| {
            view.focus_handle
                .as_ref()
                .expect("target starts mounted")
                .focus(window);
        })
        .expect("focus target should accept focus");
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .expect("focused target should render");

    assert!(
        root.read_with(cx, |root, _| {
            root.theme
                .with_focus_visible(root.input_modality.shows_focus_ring())
                .palette()
                .ring
                .a
                > 0.0
        }),
        "the mounted focused target must retain keyboard modality"
    );

    window
        .update(cx, |view, _window, cx| {
            view.focus_handle = None;
            cx.notify();
        })
        .expect("target should unmount");
    cx.run_until_parked();
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .expect("unmounted target should produce a focus-lost frame");

    assert!(
        root.read_with(cx, |root, _| {
            root.theme
                .with_focus_visible(root.input_modality.shows_focus_ring())
                .palette()
                .ring
                .a
                == 0.0
        }),
        "target unmount must clear the stale keyboard focus-ring decision"
    );
}

#[gpui::test]
async fn demo_root_materializes_shared_facts_without_a_platform_client(cx: &mut TestAppContext) {
    let window =
        cx.add_window(|_window, cx| crate::gpui_app::root::RootView::new_demo(Theme::dark(), cx));
    let view = window.entity(cx).expect("demo RootView entity");
    view.read_with(cx, |view, _| {
        assert!(
            view.platform.is_none(),
            "demo must not create a platform client"
        );
        assert!(view.telemetry_frame_state.is_ready());
        assert!(view.system_snapshot().cpu.brand.is_some());
        assert!(view.processes().len() >= 2);
        assert!(view.services().len() >= 2);
        assert!(!view.startup_entries().is_empty());
        assert!(!view.sessions().is_empty());
    });

    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .expect("demo window should render");
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let ready_body = visual
        .debug_bounds(super::TELEMETRY_READY_BODY_SELECTOR)
        .expect("demo should render the telemetry-ready body");
    assert!(
        ready_body.size.width > gpui::px(300.0) && ready_body.size.height > gpui::px(200.0),
        "demo body must have a usable surface: {ready_body:?}"
    );
    drop(visual);
}

// ── Desktop-widget surface (render-path) ─────────────────────────────────────
//
// The widget replaces the whole desktop shell, so its proof is a render-path
// window test: with the widget surface role set and a seeded typed snapshot,
// the four metric cards must paint with real bounded geometry from that
// snapshot — proving the compact projection consumes the SAME facts as the
// full dashboard rather than a second data fold.

use gpui::px;

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
        snap.memory = MemoryMetrics::from_observations(
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
