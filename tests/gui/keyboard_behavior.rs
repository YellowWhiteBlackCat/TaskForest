//! Production-root keyboard behavior not covered by pure command-router tests.

use gpui::{AppContext, Entity, Keystroke, TestAppContext, WindowHandle};
use taskmanager_application::{ConfirmationKind, SurfaceKind};
use taskmanager_core::core::SmartSelfTestKind;
use taskmanager_gpui::gpui_app::dashboard::DashboardPanel;
use taskmanager_gpui::gpui_app::root::{
    GpuiSurfaceKind, RootView, TopPage, WindowSurfaceDismissReason, WindowSurfaceKind,
};
use taskmanager_gpui::gpui_app::system_health_view::SmartSelfTestConfirmationRequest;
use taskmanager_theme::Theme;

/// The harness window root is our own RootView directly (P4 consumption switch:
/// the gc Root wrapper is gone; the LayerStack overlay host lives inside
/// RootView, so no separate overlay entity is needed here).
fn wrapped_root(cx: &mut TestAppContext) -> (WindowHandle<RootView>, Entity<RootView>) {
    let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    let view = win.entity(cx).expect("window root RootView entity");
    (win, view)
}

fn draw(cx: &mut TestAppContext, win: WindowHandle<RootView>) {
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
}

/// The focused element is a text input: read the window's key-context stack
/// (both the gc `Input` and the own `text_input` register the same "Input"
/// context on their focused element). Equivalent assertion to the old gc-Root
/// `focused_input` query — reads window focus state, not a gc overlay host.
fn input_focused(cx: &mut TestAppContext, win: WindowHandle<RootView>) -> bool {
    win.update(cx, |_root, window, _cx| {
        window
            .context_stack()
            .iter()
            .any(|context| context.contains("Input"))
    })
    .unwrap()
}

/// The focused element is inside the own modal focus scope ("TaskManagerModal"
/// is the modal_focus.rs context registered by RootView's dialog overlay).
fn taskmanager_modal_focused(cx: &mut TestAppContext, win: WindowHandle<RootView>) -> bool {
    win.update(cx, |_root, window, _cx| {
        window
            .context_stack()
            .iter()
            .any(|context| context.contains("TaskManagerModal"))
    })
    .unwrap()
}

/// A stateless production dialog must take initial focus, keep both Tab
/// directions inside its scope, close through the typed Escape route, and
/// restore the exact trigger control rather than an arbitrary global tab stop.
#[gpui::test]
async fn mc06_modal_focus_case_modal_traps_tab_and_restores_trigger_focus(cx: &mut TestAppContext) {
    let (win, view) = wrapped_root(cx);
    view.update(cx, |view, cx| {
        view.mark_telemetry_frame_ready();
        view.page = TopPage::Performance;
        cx.notify();
    });
    draw(cx, win);

    let trigger = win
        .update(cx, |_root, window, cx| {
            window.focus_next();
            window
                .focused(cx)
                .expect("the production shell exposes a first tab stop")
        })
        .unwrap();
    view.update(cx, |view, cx| {
        view.show_settings();
        cx.notify();
    });
    draw(cx, win);

    assert!(
        taskmanager_modal_focused(cx, win),
        "opening Settings must move initial focus into the modal scope"
    );
    assert!(
        !win.update(cx, |_root, window, _cx| trigger.is_focused(window))
            .unwrap(),
        "opening the modal must move focus away from the trigger"
    );

    for keystroke in ["tab", "shift-tab"] {
        for _ in 0..64 {
            cx.dispatch_keystroke(win.into(), Keystroke::parse(keystroke).unwrap());
            assert!(
                taskmanager_modal_focused(cx, win),
                "{keystroke} must wrap inside the modal instead of entering the inert page"
            );
        }
    }

    cx.dispatch_keystroke(win.into(), Keystroke::parse("escape").unwrap());
    assert!(!view.read_with(cx, |view, _cx| view.settings_open()));
    assert!(
        win.update(cx, |_root, window, _cx| trigger.is_focused(window))
            .unwrap(),
        "closing the modal must restore its exact trigger focus handle"
    );

    // Modal content actions close typed state directly rather than calling the
    // overlay's close callback. The next render must use the same restoration
    // registry instead of losing focus with the removed dialog subtree.
    view.update(cx, |view, cx| {
        view.show_settings();
        cx.notify();
    });
    draw(cx, win);
    view.update(cx, |view, cx| {
        view.dismiss_window_surface(
            WindowSurfaceKind::Settings,
            WindowSurfaceDismissReason::Cancel,
        );
        cx.notify();
    });
    draw(cx, win);
    assert!(
        win.update(cx, |_root, window, _cx| trigger.is_focused(window))
            .unwrap(),
        "a direct content-action close must also restore the exact trigger"
    );
}

/// Ctrl+F changes pages when needed and focuses the real search input; Tab must
/// then escape that input instead of trapping keyboard-only users.
#[gpui::test]
async fn mc03_apps_search_case_ctrl_f_focuses_search_and_tab_leaves_input(cx: &mut TestAppContext) {
    let (win, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Performance;
        cx.notify();
    });
    draw(cx, win);
    win.update(cx, |_root, window, _cx| window.focus_next())
        .unwrap();

    cx.dispatch_keystroke(win.into(), Keystroke::parse("ctrl-f").unwrap());
    cx.run_until_parked();
    draw(cx, win);
    assert_eq!(view.read_with(cx, |v, _cx| v.page), TopPage::Apps);
    assert!(input_focused(cx, win));

    cx.dispatch_keystroke(win.into(), Keystroke::parse("tab").unwrap());
    assert!(!input_focused(cx, win), "Tab must leave the search input");
}

// ─── TASK 2: behavior / event-wiring tests ──────────────────────────────────
//
// Drive the RootView key-handler wiring via the gpui test harness
// (`TestAppContext::dispatch_keystroke` synthesizes a KeyDown through the real
// dispatch pipeline). Each handler lives on the root div's `on_key_down` and
// mutates a `RootView` field; these tests prove the gesture reaches the handler
// and writes the field. Pixel-position mouse clicks on the gear/tabs/sidebar rows
// are NOT driven here: synthesizing a click needs the element's laid-out bounds,
// which gpui's test harness does not expose by id — guessing positions would make
// the tests flaky, so per the task's guidance those click-wiring paths are covered
// by the render+state tests above + review, and skipped here with this note.

/// Focus the first tab stop so a synthesized keystroke actually reaches RootView's
/// root `on_key_down`. gpui dispatches key events along the dispatch path from the
/// window root DOWN TO the focused node; with no focus the path is just the window
/// root and the RootView `<div id="root">` listener (a child of that root) never
/// fires. `focus_next()` from an empty focus lands on the first `.focusable()`
/// element (a titlebar tab), which is a descendant of `#root` — so the path now
/// includes `#root` and its `on_key_down` runs. Must be called AFTER at least one
/// `window.draw` so the rendered frame's tab-stop list is populated.
fn focus_first_tab_stop(cx: &mut TestAppContext, win: gpui::WindowHandle<RootView>) {
    win.update(cx, |_v, window, _cx| window.focus_next())
        .unwrap();
}

fn dispatch_control_modifier(cx: &mut TestAppContext, win: WindowHandle<RootView>, held: bool) {
    let mut visual = gpui::VisualTestContext::from_window(win.into(), cx);
    visual.simulate_modifiers_change(gpui::Modifiers {
        control: held,
        ..Default::default()
    });
}

/// Bare `Escape` dismisses the one typed surface that owns this window. Opening
/// later surfaces replaces earlier ones and cleans their render caches.
#[gpui::test]
async fn mc06_modal_cancel_case_escape_closes_open_modals(cx: &mut TestAppContext) {
    let win = cx.add_window(|_w, cx| RootView::new(Theme::dark(), cx));
    win.update(cx, |v, _w, cx| {
        v.mark_telemetry_frame_ready();
        v.show_settings();
        v.show_about();
        v.show_first_run();
        v.show_run_task();
        v.show_dashboard_panel(DashboardPanel::Events);
        v.local_feedback_toast = Some(cx.new(|cx| {
            taskmanager_ui::overlays::toast::ToastState::new(
                1,
                "unchanged",
                taskmanager_ui::overlays::toast::ToastKind::Info,
                cx,
            )
        }));
        v.request_system_health_self_test_confirmation(SmartSelfTestConfirmationRequest {
            device_id: "disk:wwid:escape-fixture".into(),
            device_generation: taskmanager_core::core::DeviceGeneration::INITIAL,
            disk_name: "nvme0n1".into(),
            disk_label: "Escape fixture".into(),
            kind: SmartSelfTestKind::Short,
        });
        assert_eq!(
            v.active_surface_kind(),
            Some(GpuiSurfaceKind::Shared(SurfaceKind::Confirmation(
                ConfirmationKind::SmartSelfTest
            )))
        );
        cx.notify();
    })
    .unwrap();
    // Draw once so the window has a laid-out scene before dispatch.
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    focus_first_tab_stop(cx, win);

    cx.dispatch_keystroke(win.into(), Keystroke::parse("escape").unwrap());

    win.read_with(cx, |v, cx| {
        assert!(!v.window_surface_open(), "ESC must release the input owner");
        assert!(
            v.system_health_confirmation().is_none(),
            "ESC must discard the SMART request without executing it"
        );
        assert_eq!(
            v.local_feedback_toast
                .as_ref()
                .map(|t| t.read(cx).message().to_string()),
            Some("unchanged".into()),
            "ESC cancellation must not execute or replace process feedback"
        );
    })
    .unwrap();
}

/// `Alt+1..6` switches the top-level page (Performance/Apps/Services/System/
/// Startup/Users). Verifies the keystroke dispatch reaches the root key handler
/// and writes `RootView::page`.
///
/// Only the Alt+digit bindings for the three Input-free pages (Performance /
/// System / Users) are driven here. Apps (Alt+2), Services (Alt+3), and Startup
/// (Alt+5) render a search input; focusing/typing into the still-gc search Input
/// expects a `gpui_component::Root` overlay host at the window root (P4 removed
/// the wrapper), so driving focus into those Inputs would panic until the gc
/// Input itself migrates. The render path for every page — including
/// Apps/Services/Startup — is already covered by
/// the capture/page render suites; this test covers the key WIRING for
/// the pages whose Inputs don't get in the way.
#[gpui::test]
async fn mc00_nav_keyboard_case_alt_digit_switches_top_page(cx: &mut TestAppContext) {
    let win = cx.add_window(|_w, cx| RootView::new(Theme::dark(), cx));
    win.update(cx, |v, _w, cx| {
        v.mark_telemetry_frame_ready();
        cx.notify();
    })
    .unwrap();
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    // Pages whose bodies render NO gpui_component Input (so focus stays on the
    // titlebar tab and each Alt+digit bubbles cleanly to the root key handler).
    let cases = [
        ("alt-4", TopPage::System),
        ("alt-6", TopPage::Users),
        ("alt-1", TopPage::Performance),
        ("alt-2", TopPage::Apps),
        // App-history (the seventh shared page) is Input-free, so Alt+7 reaches
        // the root key handler and selects it through the shared router.
        ("alt-7", TopPage::AppHistory),
    ];
    for (ks, expected) in cases {
        // Reset to the Input-free Performance page + re-focus a titlebar tab
        // before each keystroke, so every dispatch starts from the same clean
        // focus state (a page switch in the previous iteration may have moved
        // focus into a body element that would absorb the next Alt+digit).
        win.update(cx, |v, _w, cx| {
            v.page = TopPage::Performance;
            cx.notify();
        })
        .unwrap();
        cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
            .unwrap();
        focus_first_tab_stop(cx, win);
        cx.dispatch_keystroke(win.into(), Keystroke::parse(ks).unwrap());
        let got = win.read_with(cx, |v, _cx| v.page).unwrap();
        assert_eq!(
            got, expected,
            "keystroke {ks} should switch page to {expected:?}, got {got:?}"
        );
    }
}

/// `Ctrl+Space` toggles the frontend-owned refresh policy synchronously.
#[gpui::test]
async fn mc06_pause_shortcut_case_ctrl_space_toggles_pause(cx: &mut TestAppContext) {
    let win = cx.add_window(|_w, cx| RootView::new(Theme::dark(), cx));
    win.update(cx, |v, _w, cx| {
        v.mark_telemetry_frame_ready();
        cx.notify();
    })
    .unwrap();
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    focus_first_tab_stop(cx, win);

    cx.dispatch_keystroke(win.into(), Keystroke::parse("ctrl-space").unwrap());
    let after = win
        .read_with(cx, |view, _cx| view.telemetry_refresh_policy.is_paused())
        .unwrap();
    assert!(after, "Ctrl+Space must pause the local scheduler");

    cx.dispatch_keystroke(win.into(), Keystroke::parse("ctrl-space").unwrap());
    let restored = win
        .read_with(cx, |view, _cx| view.telemetry_refresh_policy.is_paused())
        .unwrap();
    assert!(!restored, "a second Ctrl+Space must resume scheduling");
}

/// Holding Ctrl pauses the complete frontend refresh gate through the real
/// GPUI modifier lifecycle; releasing it resumes unless the independent
/// Ctrl+Space/manual pause remains active.
#[gpui::test]
async fn mc06_ctrl_pause_case_holding_ctrl_pauses_and_releasing_ctrl_resumes_ui_refresh(
    cx: &mut TestAppContext,
) {
    let win = cx.add_window(|_w, cx| RootView::new(Theme::dark(), cx));
    win.update(cx, |v, _w, cx| {
        v.mark_telemetry_frame_ready();
        cx.notify();
    })
    .unwrap();
    draw(cx, win);
    focus_first_tab_stop(cx, win);

    assert!(
        !win.read_with(cx, |view, _cx| view.telemetry_refresh_policy.is_paused())
            .unwrap()
    );

    dispatch_control_modifier(cx, win, true);
    assert!(
        win.read_with(cx, |view, _cx| {
            view.telemetry_refresh_policy.is_control_held()
                && view.telemetry_refresh_policy.is_paused()
        })
        .unwrap()
    );

    dispatch_control_modifier(cx, win, false);
    assert!(
        win.read_with(cx, |view, _cx| {
            !view.telemetry_refresh_policy.is_control_held()
                && !view.telemetry_refresh_policy.is_paused()
        })
        .unwrap()
    );

    // The transient modifier state must not erase a user-selected manual
    // pause: release Ctrl only removes its own pause reason.
    dispatch_control_modifier(cx, win, true);
    cx.dispatch_keystroke(win.into(), Keystroke::parse("ctrl-space").unwrap());
    dispatch_control_modifier(cx, win, false);
    assert!(
        win.read_with(cx, |view, _cx| {
            view.telemetry_refresh_policy.is_manually_paused()
                && view.telemetry_refresh_policy.is_paused()
        })
        .unwrap()
    );
}

/// F9 toggles the per-window Performance device navigator and leaves the page
/// renderable in both states. The command must not alter telemetry or persisted
/// device preferences; only the RootView layout projection changes.
#[gpui::test]
async fn mc05_sidebar_keyboard_case_f9_toggles_sidebar_visibility(cx: &mut TestAppContext) {
    let win = cx.add_window(|_w, cx| RootView::new(Theme::dark(), cx));
    win.update(cx, |v, _w, cx| {
        v.mark_telemetry_frame_ready();
        cx.notify();
    })
    .unwrap();
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    focus_first_tab_stop(cx, win);

    let initially_visible = win.read_with(cx, |view, _cx| view.sidebar_visible).unwrap();
    assert!(initially_visible, "the device sidebar starts visible");

    cx.dispatch_keystroke(win.into(), Keystroke::parse("f9").unwrap());
    let hidden = win.read_with(cx, |view, _cx| view.sidebar_visible).unwrap();
    assert!(!hidden, "F9 must hide the device sidebar");
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    cx.dispatch_keystroke(win.into(), Keystroke::parse("f9").unwrap());
    let restored = win.read_with(cx, |view, _cx| view.sidebar_visible).unwrap();
    assert!(restored, "a second F9 must restore the device sidebar");
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
}

/// Mission Center's System Information shortcut is a real GPUI command path:
/// Ctrl+A opens the per-window dialog and Escape closes it without touching the
/// correlated telemetry read model.
#[gpui::test]
async fn mc06_system_about_case_ctrl_a_opens_and_escape_closes_system_information(
    cx: &mut TestAppContext,
) {
    let win = cx.add_window(|_w, cx| RootView::new(Theme::dark(), cx));
    win.update(cx, |view, _window, cx| {
        view.mark_telemetry_frame_ready();
        cx.notify();
    })
    .unwrap();
    draw(cx, win);
    focus_first_tab_stop(cx, win);

    cx.dispatch_keystroke(win.into(), Keystroke::parse("ctrl-a").unwrap());
    assert!(
        win.read_with(cx, |view, _cx| view.window_surface_kind())
            .unwrap()
            == Some(WindowSurfaceKind::SystemAbout),
        "Ctrl+A must open the System Information dialog"
    );
    draw(cx, win);

    cx.dispatch_keystroke(win.into(), Keystroke::parse("escape").unwrap());
    assert!(
        win.read_with(cx, |view, _cx| view.window_surface_kind())
            .unwrap()
            != Some(WindowSurfaceKind::SystemAbout),
        "Escape must close the System Information dialog"
    );
}

/// `F5` triggers a manual process-list refresh. It must not panic and must leave
/// `procs` as a (refreshed) Vec — exercising the refresh handler end to end.
#[gpui::test]
async fn f5_refreshes_process_list(cx: &mut TestAppContext) {
    let win = cx.add_window(|_w, cx| RootView::new(Theme::dark(), cx));
    win.update(cx, |v, _w, cx| {
        v.mark_telemetry_frame_ready();
        cx.notify();
    })
    .unwrap();
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    focus_first_tab_stop(cx, win);

    cx.dispatch_keystroke(win.into(), Keystroke::parse("f5").unwrap());

    // F5 must keep the view renderable through the refresh path: re-draw the
    // window and confirm it stays coherent instead of panicking. An exact host
    // process count is not portable; render-coherence after refresh is.
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
}
