use gpui::{AppContext, Keystroke, TestAppContext, VisualTestContext, WindowHandle};

use crate::gpui_app::root::{RootView, TopPage};
use taskmanager_core::core::process::ProcessLiveKey;
use taskmanager_theme::Theme;

fn root(cx: &mut TestAppContext) -> WindowHandle<RootView> {
    let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    win.update(cx, |view, _window, cx| {
        view.mark_telemetry_frame_ready();
        cx.notify();
    })
    .unwrap();
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    // gpui dispatches key events down from the window root TO the focused
    // node; focusing the first tab stop (a titlebar tab under `#root`)
    // puts the root `on_key_down` handler on the dispatch path.
    win.update(cx, |_view, window, _cx| window.focus_next())
        .unwrap();
    win
}

fn collecting_root(cx: &mut TestAppContext) -> WindowHandle<RootView> {
    let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    win.update(cx, |_view, window, _cx| window.focus_next())
        .unwrap();
    win
}

fn help_open(cx: &mut TestAppContext, win: WindowHandle<RootView>) -> bool {
    win.read_with(cx, |view, _cx| view.help_open()).unwrap()
}

/// F1 (bare) toggles the help overlay through the real key-dispatch
/// pipeline, and the modal body actually renders the shared command rows
/// (a `tm-help-cmd:*` render-geometry marker appears for the Ctrl+F row).
#[gpui::test]
async fn f1_toggles_help_overlay_and_renders_shared_command_rows(cx: &mut TestAppContext) {
    let win = root(cx);
    assert!(!help_open(cx, win), "help starts closed");

    cx.dispatch_keystroke(win.into(), Keystroke::parse("f1").unwrap());
    assert!(help_open(cx, win), "F1 must open the help overlay");

    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    assert!(
        vcx.debug_bounds("tm-help-cmd:Ctrl+F").is_some(),
        "the shared Ctrl+F (Search) command row must render inside the modal"
    );
    assert!(
        vcx.debug_bounds("tm-help-page:Alt+1").is_some(),
        "the seven-page navigation section must render page rows"
    );
    drop(vcx);

    cx.dispatch_keystroke(win.into(), Keystroke::parse("f1").unwrap());
    assert!(
        !help_open(cx, win),
        "a second F1 must close the help overlay (toggle)"
    );
}

/// `?` (the shell's frontend-local help chord, keys.rs) toggles the same
/// overlay: parsed as `shift-?` here, delivered as key `"?"` on
/// Linux/Wayland (the xkb keysym name).
#[gpui::test]
async fn question_toggles_help_overlay(cx: &mut TestAppContext) {
    let win = root(cx);
    cx.dispatch_keystroke(win.into(), Keystroke::parse("shift-?").unwrap());
    assert!(help_open(cx, win), "? must open the help overlay");
    cx.dispatch_keystroke(win.into(), Keystroke::parse("shift-?").unwrap());
    assert!(
        !help_open(cx, win),
        "a second ? must close the help overlay"
    );
}

#[gpui::test]
async fn warmup_mask_blocks_global_shortcuts_until_the_first_frame(cx: &mut TestAppContext) {
    let win = collecting_root(cx);

    cx.dispatch_keystroke(win.into(), Keystroke::parse("f1").unwrap());
    cx.dispatch_keystroke(win.into(), Keystroke::parse("alt-2").unwrap());
    cx.dispatch_keystroke(win.into(), Keystroke::parse("ctrl-space").unwrap());

    let (help_open, page, paused) = win
        .read_with(cx, |view, _cx| {
            (
                view.help_open(),
                view.page,
                view.telemetry_refresh_policy.is_paused(),
            )
        })
        .unwrap();
    assert!(!help_open, "warm-up must not open help");
    assert_eq!(page, TopPage::Performance, "warm-up must not change page");
    assert!(!paused, "warm-up must not pause collection");
}

/// ESC closes the help overlay through the shared Dismiss path, and
/// neither opening nor closing disturbs the active page.
#[gpui::test]
async fn escape_closes_help_overlay_without_disturbing_the_page(cx: &mut TestAppContext) {
    let win = root(cx);
    win.update(cx, |view, _window, cx| {
        view.page = TopPage::Services;
        cx.notify();
    })
    .unwrap();
    cx.dispatch_keystroke(win.into(), Keystroke::parse("f1").unwrap());
    assert!(help_open(cx, win));
    assert_eq!(
        win.read_with(cx, |view, _cx| view.page).unwrap(),
        TopPage::Services,
        "opening help must not switch the page"
    );

    cx.dispatch_keystroke(win.into(), Keystroke::parse("escape").unwrap());
    assert!(!help_open(cx, win), "ESC must close the help overlay");
    assert_eq!(
        win.read_with(cx, |view, _cx| view.page).unwrap(),
        TopPage::Services,
        "closing help must restore the untouched page"
    );
}

/// While a text input is focused, `?` types into it instead of opening the
/// help overlay (the same precedence the shared router gives inputs); F1
/// stays available because no input consumes it.
#[gpui::test]
async fn question_is_suppressed_while_a_text_input_is_focused(cx: &mut TestAppContext) {
    let win = root(cx);
    win.update(cx, |view, _window, cx| {
        view.page = TopPage::Apps;
        view.replace_processes_for_test(vec![
            taskmanager_test_support::ProcessItemFixtureBuilder::from_item(
                taskmanager_core::core::process::ProcessItem::default(),
            )
            .pid(42)
            .name("searchable".into())
            .build(),
        ]);
        cx.notify();
    })
    .unwrap();
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    cx.dispatch_keystroke(win.into(), Keystroke::parse("ctrl-f").unwrap());
    cx.run_until_parked();
    let input_focused = win
        .update(cx, |_view, window, _cx| {
            window
                .context_stack()
                .iter()
                .any(|context| context.contains("Input"))
        })
        .unwrap();
    assert!(input_focused, "Ctrl+F must focus the Apps search input");

    cx.dispatch_keystroke(win.into(), Keystroke::parse("shift-?").unwrap());
    assert!(
        !help_open(cx, win),
        "? must type into the focused input instead of opening help"
    );

    cx.dispatch_keystroke(win.into(), Keystroke::parse("f1").unwrap());
    assert!(
        help_open(cx, win),
        "F1 stays available while an input is focused"
    );
}

/// Shell PageChanged semantics (crates/taskmanager-shell `apply_ui_effect`,
/// fired on every SelectPage): a page-navigation chord closes the help
/// overlay AND any pending high-blast-radius confirmations while still
/// switching the page — the same cleanup the TUI/iced shell performs, so a
/// shortcut learned from the help sheet navigates instead of piling a
/// modal on top of the new page.
#[gpui::test]
async fn page_navigation_closes_help_and_pending_confirmations(cx: &mut TestAppContext) {
    let win = root(cx);
    win.update(cx, |view, _window, cx| {
        view.replace_processes_for_test(vec![
            taskmanager_test_support::ProcessItemFixtureBuilder::from_item(
                taskmanager_core::core::process::ProcessItem::default(),
            )
            .pid(42)
            .name("target".into())
            .scalar_observations(taskmanager_core::core::process::ProcessScalarObservations {
                start_token: taskmanager_core::core::ScalarObservation::available(4_200, 1),
                ..Default::default()
            })
            .build(),
        ]);
        cx.notify();
    })
    .unwrap();

    // Help open + a page chord: the overlay closes and the page switches.
    cx.dispatch_keystroke(win.into(), Keystroke::parse("f1").unwrap());
    assert!(help_open(cx, win), "F1 must open the help overlay first");
    cx.dispatch_keystroke(win.into(), Keystroke::parse("alt-2").unwrap());
    assert!(
        !help_open(cx, win),
        "a page chord must close the help overlay"
    );
    assert_eq!(
        win.read_with(cx, |view, _cx| view.page).unwrap(),
        TopPage::Apps,
        "the chord must still switch the page"
    );

    // Pending end-task confirmation + a page chord: the confirmation is
    // dismissed without executing any process control.
    win.update(cx, |view, _window, cx| {
        view.request_end_task_confirmation(
            ProcessLiveKey::from_parts(42, 4_200).expect("fixture identity"),
        );
        assert!(
            matches!(
                view.pending_confirmation(),
                Some(taskmanager_application::PendingConfirmation::EndTask(_))
            ),
            "the request must stage a confirmation"
        );
        cx.notify();
    })
    .unwrap();
    cx.dispatch_keystroke(win.into(), Keystroke::parse("alt-3").unwrap());
    assert!(
        win.read_with(cx, |view, _cx| {
            view.pending_confirmation().is_none() && view.page == TopPage::Services
        })
        .unwrap(),
        "a page chord must dismiss the pending confirmation and navigate"
    );
}

#[gpui::test]
async fn f9_toggles_sidebar_visibility(cx: &mut TestAppContext) {
    let win = root(cx);
    assert!(
        win.read_with(cx, |view, _cx| view.sidebar_visible).unwrap(),
        "sidebar starts visible"
    );

    cx.dispatch_keystroke(win.into(), Keystroke::parse("f9").unwrap());
    assert!(
        !win.read_with(cx, |view, _cx| view.sidebar_visible).unwrap(),
        "F9 must toggle sidebar visibility off"
    );

    cx.dispatch_keystroke(win.into(), Keystroke::parse("f9").unwrap());
    assert!(
        win.read_with(cx, |view, _cx| view.sidebar_visible).unwrap(),
        "a second F9 must toggle sidebar visibility back on"
    );
}

#[gpui::test]
async fn ctrl_space_toggles_telemetry_pause(cx: &mut TestAppContext) {
    let win = root(cx);
    assert!(
        !win.read_with(cx, |view, _cx| view
            .telemetry_refresh_policy
            .is_manually_paused())
            .unwrap(),
        "telemetry starts unpaused"
    );

    cx.dispatch_keystroke(win.into(), Keystroke::parse("ctrl-space").unwrap());
    assert!(
        win.read_with(cx, |view, _cx| view
            .telemetry_refresh_policy
            .is_manually_paused())
            .unwrap(),
        "Ctrl+Space must pause telemetry collection"
    );

    cx.dispatch_keystroke(win.into(), Keystroke::parse("ctrl-space").unwrap());
    assert!(
        !win.read_with(cx, |view, _cx| view
            .telemetry_refresh_policy
            .is_manually_paused())
            .unwrap(),
        "a second Ctrl+Space must unpause telemetry collection"
    );
}

#[gpui::test]
async fn alt_digits_navigate_pages(cx: &mut TestAppContext) {
    let win = root(cx);
    let cases = [
        ("alt-1", TopPage::Performance),
        ("alt-4", TopPage::System),
        ("alt-6", TopPage::Users),
        ("alt-7", TopPage::AppHistory),
        ("alt-2", TopPage::Apps),
    ];
    for (shortcut, expected) in cases {
        cx.dispatch_keystroke(win.into(), Keystroke::parse(shortcut).unwrap());
        assert_eq!(
            win.read_with(cx, |view, _cx| view.page).unwrap(),
            expected,
            "{shortcut} must navigate to {expected:?}"
        );
    }
}

#[gpui::test]
async fn escape_dismisses_confirmation_modal(cx: &mut TestAppContext) {
    let win = root(cx);
    win.update(cx, |view, _window, cx| {
        view.replace_processes_for_test(vec![
            taskmanager_test_support::ProcessItemFixtureBuilder::from_item(
                taskmanager_core::core::process::ProcessItem::default(),
            )
            .pid(42)
            .name("target".into())
            .scalar_observations(taskmanager_core::core::process::ProcessScalarObservations {
                start_token: taskmanager_core::core::ScalarObservation::available(4_200, 1),
                ..Default::default()
            })
            .build(),
        ]);
        view.request_end_task_confirmation(
            ProcessLiveKey::from_parts(42, 4_200).expect("fixture identity"),
        );
        assert!(view.pending_confirmation().is_some());
        cx.notify();
    })
    .unwrap();

    cx.dispatch_keystroke(win.into(), Keystroke::parse("escape").unwrap());
    assert!(
        win.read_with(cx, |view, _cx| view.pending_confirmation().is_none())
            .unwrap(),
        "Escape must dismiss the pending confirmation"
    );
}

#[gpui::test]
async fn f5_refreshes_processes_without_panic(cx: &mut TestAppContext) {
    let win = root(cx);
    cx.dispatch_keystroke(win.into(), Keystroke::parse("f5").unwrap());
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    assert!(
        win.read_with(cx, |view, _cx| !view.telemetry_frame_state.is_collecting())
            .unwrap()
    );
}
