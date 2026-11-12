use super::{CursorRefreshState, should_schedule_cursor_refresh, ui_font_with_fallback};
use gpui::{
    AppContext, Context, FocusHandle, InteractiveElement, IntoElement, ParentElement, Render,
    Styled, TestAppContext, VisualTestContext, Window, div,
};
use taskforest_core::core::metrics::MemoryMetrics;
use taskmanager_shell::presentation::ALERT_MARKER;
use taskmanager_theme::{FONT_MISANS_VF, FONT_ROBOTO_MONO, Theme};
use taskmanager_ui_contract::IconId;
use taskmanager_ui_contract::InputModality;
use zhuge_ui_tokens::StatusKind;

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
fn shell_feedback_severity_wears_the_shared_status_glyph() {
    use taskmanager_icons::status_icon;
    use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource, ShellApp};

    // The surface answers the glyph question by DELEGATING: the icon
    // registry owns which severity wears which glyph and the shell owns the
    // severity → status projection. A local mapping here would be the fork
    // this contract forbids.
    for (severity, glyph) in [
        (FeedbackSeverity::Info, IconId::Health),
        (FeedbackSeverity::Success, IconId::CircleCheck),
        (FeedbackSeverity::Warning, IconId::TriangleAlert),
        (FeedbackSeverity::Error, IconId::Alert),
    ] {
        let mut shell = ShellApp::new();
        shell.report_notice(
            FeedbackSource::Interaction,
            severity,
            FeedbackLifecycle::UntilReplaced,
            "status",
        );
        let notice = shell.feedback_notice().expect("a notice is mounted");
        assert_eq!(
            super::transients::feedback_status_icon(notice),
            status_icon(severity.status_kind()),
            "{severity:?} wears the shared status glyph"
        );
        assert_eq!(super::transients::feedback_status_icon(notice), glyph);
    }
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

/// Focus-visible is a modality decision, not a focus decision: `FocusHandle`
/// carries no input origin, so the per-window state machine
/// is the sole source of the ring policy. The capture listeners call these
/// exact transitions before descendants handle the same event, and the render
/// snapshot consumes `shows_focus_ring()` through `Theme::with_focus_visible`.
#[test]
fn input_modality_only_keyboard_paints_the_focus_ring() {
    let mut modality = InputModality::default();
    assert!(
        !modality.shows_focus_ring(),
        "programmatic focus must stay calm (initial state)"
    );

    assert!(
        modality.transition(InputModality::Keyboard),
        "the keyboard origin must be a real transition"
    );
    assert!(
        modality.shows_focus_ring(),
        "keyboard navigation must paint the ring"
    );
    assert!(
        !modality.transition(InputModality::Keyboard),
        "a repeated key must not churn the render"
    );

    assert!(
        modality.transition(InputModality::Pointer),
        "the pointer origin must be a real transition"
    );
    assert!(
        !modality.shows_focus_ring(),
        "pointer-driven focus must suppress the ring"
    );
    assert!(
        !modality.transition(InputModality::Pointer),
        "a repeated press must not churn the render"
    );

    assert!(
        modality.transition(InputModality::Keyboard),
        "the next keyboard origin must ring again"
    );
    assert!(modality.shows_focus_ring());
}

#[gpui::test]
async fn input_modality_clears_when_its_window_blurs(cx: &mut TestAppContext) {
    let window =
        cx.add_window(|_window, cx| crate::gpui_app::root::RootView::new_demo(Theme::dark(), cx));
    let root = window.entity(cx).expect("demo RootView entity");
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
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
    visual.update(|window, cx| window.draw(cx).clear(cx));
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
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
        .expect("focus-target harness should render");

    window
        .update(cx, |_view, window, cx| {
            root.update(cx, |root, cx| {
                assert!(root.input_modality.transition(InputModality::Keyboard));
                root.ensure_input_modality_observers(window, cx);
            });
        })
        .expect("focus lifecycle window should update");
    window
        .update(cx, |view, window, cx| {
            view.focus_handle
                .as_ref()
                .expect("target starts mounted")
                .focus(window, cx);
        })
        .expect("focus target should accept focus");
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
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
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
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

    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
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

use taskforest_core::core::metrics::{
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
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear(cx))
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
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear(cx))
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
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    view.read_with(cx, |v, _| {
        assert!(
            v.local_feedback_toast.is_some(),
            "a refused Native preference must surface an honest notice"
        );
        assert!(v.decoration_outcome_reported, "the notice must latch");
        assert_eq!(v.local_feedback_seq, 1);
    });

    // The severity is a TYPED fact the caller declared, and the copy carries
    // no marker: the compositor refused the frame but the app adapted, so
    // the notice is a caution, not a failure — and the icon the severity
    // selects is the only marker the user sees.
    view.read_with(cx, |v, _| {
        let toast = v
            .local_feedback_toast
            .as_ref()
            .expect("the refused preference raised a notice");
        assert_eq!(
            toast.read_with(cx, |toast, _| toast.kind()),
            StatusKind::Warning,
            "a refused frame preference is a caution, not a failure"
        );
        assert!(
            !toast
                .read_with(cx, |toast, _| toast.message().to_string())
                .starts_with(ALERT_MARKER),
            "the copy must not repeat the marker the icon already carries"
        );
    });

    // The latch holds: subsequent renders do not re-notify (no toast churn).
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear(cx))
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
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    view.read_with(cx, |v, _| {
        assert!(!v.decoration_outcome_reported);
        assert_eq!(
            v.local_feedback_seq, 1,
            "System mode must never raise an outcome notice"
        );
    });
}

/// Feedback occupies the CSD titlebar's outer side slot, leaving the title,
/// navigation strip and page geometry untouched at normal and compact sizes.
#[gpui::test]
async fn feedback_overlay_does_not_move_the_body_and_stays_inside_the_viewport(
    cx: &mut TestAppContext,
) {
    use gpui::{VisualTestContext, px, size};
    use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource};

    for (width, height) in [(1180.0, 780.0), (720.0, 480.0)] {
        let win =
            cx.add_window(|_window, cx| crate::gpui_app::root::RootView::new(Theme::dark(), cx));
        cx.simulate_window_resize(win.into(), size(px(width), px(height)));
        win.update(cx, |view, _window, cx| {
            view.decorations_override = Some(false);
            view.mark_telemetry_frame_ready();
            cx.notify();
        })
        .unwrap();
        cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();

        let body_before = {
            let mut visual = VisualTestContext::from_window(win.into(), cx);
            visual
                .debug_bounds(super::TELEMETRY_READY_BODY_SELECTOR)
                .expect("the ready body must render before feedback")
        };

        win.update(cx, |view, _window, cx| {
            view.shell.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Success,
                FeedbackLifecycle::TIMED_SHORT,
                format!(
                    "Current window saved: /run/media/{}/very-long-workspace/target/host-wayland-diagnostic/runs/{}/capture.png (1180x780)",
                    "capture-user", "977f44d34-bc94-4c7d-9d89-0f6a6b0a62e9"
                ),
            );
            cx.notify();
        })
        .unwrap();
        cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();

        let mut visual = VisualTestContext::from_window(win.into(), cx);
        let body_after = visual
            .debug_bounds(super::TELEMETRY_READY_BODY_SELECTOR)
            .expect("the ready body must remain rendered with feedback");
        let titlebar = visual
            .debug_bounds("tm-feedback-titlebar")
            .expect("the client titlebar must render");
        let title = visual
            .debug_bounds("tm-window-title")
            .expect("the centered window title must render");
        let navigation = visual
            .debug_bounds("tm-navigation-strip")
            .expect("the navigation strip must render");
        let overlay = visual
            .debug_bounds("tm-feedback-overlay")
            .expect("feedback must mount in the deferred titlebar slot");

        assert_eq!(
            body_after, body_before,
            "mounting feedback must not move or resize the page body"
        );
        assert!(
            overlay.origin.x >= px(0.0) && overlay.right() <= px(width),
            "the feedback overlay must stay inside the viewport: {overlay:?}"
        );
        assert!(
            overlay.origin.y >= titlebar.origin.y && overlay.bottom() <= titlebar.bottom(),
            "the one-line feedback card must fit the titlebar: card={overlay:?}, bar={titlebar:?}"
        );
        let overlaps_title = overlay.origin.x < title.right()
            && overlay.right() > title.origin.x
            && overlay.origin.y < title.bottom()
            && overlay.bottom() > title.origin.y;
        assert!(
            !overlaps_title,
            "feedback must stay clear of the centered window title: card={overlay:?}, title={title:?}"
        );
        assert!(
            overlay.bottom() <= navigation.origin.y,
            "feedback must not cover the navigation strip: card={overlay:?}, nav={navigation:?}"
        );
    }
}

/// Native-decorated windows use the app navigation strip as the available
/// chrome slot; the toast must finish above the page body there as well.
#[gpui::test]
async fn feedback_overlay_stays_in_native_navigation_chrome(cx: &mut TestAppContext) {
    use gpui::{VisualTestContext, px, size};
    use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource};

    let win = cx.add_window(|_window, cx| crate::gpui_app::root::RootView::new(Theme::dark(), cx));
    cx.simulate_window_resize(win.into(), size(px(1180.0), px(780.0)));
    win.update(cx, |view, _window, cx| {
        view.decorations_override = Some(true);
        view.mark_telemetry_frame_ready();
        cx.notify();
    })
    .unwrap();
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    let body_before = {
        let mut visual = VisualTestContext::from_window(win.into(), cx);
        visual
            .debug_bounds(super::TELEMETRY_READY_BODY_SELECTOR)
            .expect("the ready body must render before feedback")
    };

    win.update(cx, |view, _window, cx| {
        view.shell.report_notice(
            FeedbackSource::Persistence,
            FeedbackSeverity::Success,
            FeedbackLifecycle::TIMED_SHORT,
            "Current window saved: /workspace/archive/capture.png",
        );
        cx.notify();
    })
    .unwrap();
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();

    let mut visual = VisualTestContext::from_window(win.into(), cx);
    let body_after = visual
        .debug_bounds(super::TELEMETRY_READY_BODY_SELECTOR)
        .expect("the ready body must remain rendered with feedback");
    let navigation = visual
        .debug_bounds("tm-navigation-strip")
        .expect("the navigation strip must render");
    let overlay = visual
        .debug_bounds("tm-feedback-overlay")
        .expect("feedback must mount in the native navigation slot");

    assert_eq!(
        body_after, body_before,
        "mounting feedback must not move or resize the page body"
    );
    assert!(
        overlay.origin.y >= navigation.origin.y
            && overlay.bottom() <= navigation.bottom()
            && overlay.bottom() <= body_after.origin.y,
        "native-window feedback must stay in chrome above the page body: card={overlay:?}, nav={navigation:?}, body={body_after:?}"
    );
}

#[test]
fn feedback_notice_uses_the_shared_toast_role_across_primary_themes() {
    use gpui::Styled;
    use taskmanager_theme::{
        GlassSurfaceRole, HighContrast, LightDark, ResolvedFonts, Skin, tokens,
    };
    use taskmanager_ui::theme_binding::{glass_fill, glass_shadow, rem_length};

    for skin in Skin::ALL {
        for mode in LightDark::ALL {
            for high_contrast in [HighContrast::Off, HighContrast::On] {
                let theme =
                    Theme::build(skin, mode, high_contrast, ResolvedFonts::system_for(skin));
                let tier = GlassSurfaceRole::Toast.tier();
                let expected_radius: gpui::AbsoluteLength =
                    rem_length(tokens::card_radius(&theme)).into();
                let mut surface = super::transients::feedback_notice_surface(&theme).render();

                assert_eq!(
                    surface.style().background.as_ref(),
                    Some(&glass_fill(&theme, tier)),
                    "feedback must resolve through the shared Toast glass role"
                );
                assert_eq!(
                    surface.style().border_widths.top,
                    Some(gpui::AbsoluteLength::Rems(gpui::Rems(
                        tier.edge_width_rem()
                    ))),
                    "feedback must keep the shared glass hairline"
                );
                assert_eq!(
                    surface.style().corner_radii.top_left,
                    Some(expected_radius),
                    "feedback keeps its existing card geometry"
                );
                assert_eq!(
                    surface.style().box_shadow.as_ref(),
                    Some(&glass_shadow(&theme, tier)),
                    "a transient toast floats above the page"
                );
            }
        }
    }
}

/// A compact native-window rail has no horizontal titlebar slot. In that
/// layout feedback collapses to its severity indicator inside the rail.
#[gpui::test]
async fn feedback_indicator_stays_inside_native_vertical_navigation_rail(cx: &mut TestAppContext) {
    use gpui::{VisualTestContext, px, size};
    use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource};
    use taskmanager_ui_contract::navigation::NavOrientation;

    let win = cx.add_window(|_window, cx| crate::gpui_app::root::RootView::new(Theme::dark(), cx));
    cx.simulate_window_resize(win.into(), size(px(720.0), px(480.0)));
    win.update(cx, |view, _window, cx| {
        view.decorations_override = Some(true);
        view.nav_orientation = NavOrientation::Vertical;
        view.mark_telemetry_frame_ready();
        cx.notify();
    })
    .unwrap();
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    let body_before = {
        let mut visual = VisualTestContext::from_window(win.into(), cx);
        visual
            .debug_bounds(super::TELEMETRY_READY_BODY_SELECTOR)
            .expect("the ready body must render before feedback")
    };

    win.update(cx, |view, _window, cx| {
        view.shell.report_notice(
            FeedbackSource::Persistence,
            FeedbackSeverity::Success,
            FeedbackLifecycle::TIMED_SHORT,
            "Current window saved: /workspace/archive/capture.png",
        );
        cx.notify();
    })
    .unwrap();
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();

    let mut visual = VisualTestContext::from_window(win.into(), cx);
    let body_after = visual
        .debug_bounds(super::TELEMETRY_READY_BODY_SELECTOR)
        .expect("the ready body must remain rendered with feedback");
    let rail = visual
        .debug_bounds("tm-navigation-rail")
        .expect("the vertical navigation rail must render");
    let overlay = visual
        .debug_bounds("tm-feedback-overlay")
        .expect("the feedback indicator must mount in the navigation rail");

    assert_eq!(
        body_after, body_before,
        "mounting feedback must not move or resize the page body"
    );
    assert!(
        overlay.origin.x >= rail.origin.x
            && overlay.right() <= rail.right()
            && overlay.right() <= body_after.origin.x,
        "feedback must remain inside the rail and clear of the page body: indicator={overlay:?}, rail={rail:?}, body={body_after:?}"
    );
}
