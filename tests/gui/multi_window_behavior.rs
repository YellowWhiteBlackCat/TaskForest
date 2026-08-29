//! Multi-window sequence behavior for the shared overlay/focus machinery.
//!
//! Production builds one RootView per window, and each window hosts its own
//! per-dialog `LayerStack` (`dialog_overlay` keys host state via
//! `window.use_state`) and its own modal-focus scope (the `DialogFocusRegistry`
//! keeps a per-window state map). These tests pin the cross-window contract:
//! opening a dialog or popup in one window must never trap the other window's
//! Tab cycle, steal its focus, or dismiss through the other window's Escape.

use gpui::{
    AppContext, Entity, Keystroke, TestAppContext, VisualTestContext, WindowHandle, point, px,
};
use taskmanager_gpui::gpui_app::root::{RootView, TopPage};
use taskmanager_theme::Theme;

#[path = "multi_window_behavior/settings.rs"]
mod settings;

/// The harness window root is our own RootView directly (the LayerStack
/// overlay host lives inside RootView; no separate overlay entity is needed).
pub(super) fn wrapped_root(cx: &mut TestAppContext) -> (WindowHandle<RootView>, Entity<RootView>) {
    let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    let view = win.entity(cx).expect("window root RootView entity");
    (win, view)
}

pub(super) fn draw(cx: &mut TestAppContext, win: WindowHandle<RootView>) {
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
}

/// Advance past the 100ms search debounce and run any pending timer tasks, so
/// tests asserting on the debounced query can read the settled value.
fn settle_search_debounce(cx: &mut TestAppContext) {
    // gpui::Timer is smol::Timer (wall clock): sleep past the debounce for
    // real, then drain the executor so the spawned update task runs before
    // the assertions below.
    std::thread::sleep(std::time::Duration::from_millis(150));
    cx.executor().run_until_parked();
}

/// Prime a window into a clean, Input-free page so keyboard dispatch paths are
/// stable across both windows.
fn prime(cx: &mut TestAppContext, view: &Entity<RootView>) {
    view.update(cx, |view, cx| {
        view.mark_telemetry_frame_ready();
        view.page = TopPage::Performance;
        cx.notify();
    });
}

/// The focused element is inside this window's own modal focus scope.
fn taskmanager_modal_focused(cx: &mut TestAppContext, win: WindowHandle<RootView>) -> bool {
    win.update(cx, |_root, window, _cx| {
        window
            .context_stack()
            .iter()
            .any(|context| context.contains("TaskManagerModal"))
    })
    .unwrap()
}

fn focused_handle(
    cx: &mut TestAppContext,
    win: WindowHandle<RootView>,
) -> Option<gpui::FocusHandle> {
    win.update(cx, |_root, window, cx| window.focused(cx))
        .unwrap()
}

/// Click the center of this window's own `tm-text-input` element (the Apps
/// search box or the Run dialog's command field, whichever is rendered),
/// giving it keyboard focus the way a user would.
fn click_own_text_input(cx: &mut TestAppContext, win: WindowHandle<RootView>) {
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let bounds = vcx
        .debug_bounds("tm-text-input")
        .expect("the window renders a text input");
    vcx.simulate_click(bounds.center(), Default::default());
}

/// Scenario (a): a dialog opened in window A must not steal window B's focus,
/// must not trap B's Tab cycle, and must not be dismissible through B's Escape.
#[gpui::test]
async fn dialog_in_one_window_leaves_the_other_window_focus_independent(cx: &mut TestAppContext) {
    let (win_a, view_a) = wrapped_root(cx);
    let (win_b, view_b) = wrapped_root(cx);
    prime(cx, &view_a);
    prime(cx, &view_b);
    draw(cx, win_a);
    draw(cx, win_b);

    // A: focus the trigger, then open Settings.
    let trigger_a = win_a
        .update(cx, |_root, window, cx| {
            window.focus_next();
            window
                .focused(cx)
                .expect("window A exposes a first tab stop")
        })
        .unwrap();
    view_a.update(cx, |view, cx| {
        view.show_settings();
        cx.notify();
    });
    draw(cx, win_a);

    assert!(
        taskmanager_modal_focused(cx, win_a),
        "opening Settings in A must trap A's focus in its modal scope"
    );
    assert!(
        !taskmanager_modal_focused(cx, win_b),
        "A's modal scope must not leak into B's key context"
    );

    // B gains its own focus: it must land on B's first tab stop, never inside
    // A's dialog, and its Tab cycle must advance normally (not wrap inside A's
    // trap).
    win_b
        .update(cx, |_root, window, _cx| window.focus_next())
        .unwrap();
    assert!(
        !taskmanager_modal_focused(cx, win_b),
        "B's first tab stop must not be inside A's modal scope"
    );
    let b_before = focused_handle(cx, win_b).expect("B now has a focused tab stop");
    cx.dispatch_keystroke(win_b.into(), Keystroke::parse("tab").unwrap());
    let b_after = focused_handle(cx, win_b);
    assert_ne!(
        b_after.as_ref(),
        Some(&b_before),
        "Tab in B must advance B's own focus cycle"
    );
    assert!(
        !taskmanager_modal_focused(cx, win_b),
        "B's Tab cycle must never enter A's modal trap"
    );
    assert!(
        taskmanager_modal_focused(cx, win_a),
        "A's dialog must stay open and focused while B navigates"
    );

    // B's Escape must not dismiss A's dialog.
    cx.dispatch_keystroke(win_b.into(), Keystroke::parse("escape").unwrap());
    assert!(
        view_a.read_with(cx, |view, _cx| view.settings_open()),
        "Escape in B must not close A's dialog"
    );
    assert!(
        taskmanager_modal_focused(cx, win_a),
        "A's modal scope must survive B's Escape"
    );

    // A's own Escape still closes the dialog and restores A's exact trigger.
    cx.dispatch_keystroke(win_a.into(), Keystroke::parse("escape").unwrap());
    assert!(
        !view_a.read_with(cx, |view, _cx| view.settings_open()),
        "Escape in A must close A's dialog"
    );
    assert!(
        win_a
            .update(cx, |_root, window, _cx| trigger_a.is_focused(window))
            .unwrap(),
        "closing A's modal must restore A's exact trigger handle"
    );
    assert!(
        !view_b.read_with(cx, |view, _cx| view.settings_open()),
        "B never opened a dialog"
    );
}

/// Both windows may open their own dialog simultaneously; each window's Escape
/// closes only its own dialog, and each window restores its own trigger.
#[gpui::test]
async fn each_window_closes_only_its_own_dialog(cx: &mut TestAppContext) {
    let (win_a, view_a) = wrapped_root(cx);
    let (win_b, view_b) = wrapped_root(cx);
    prime(cx, &view_a);
    prime(cx, &view_b);
    draw(cx, win_a);
    draw(cx, win_b);

    let trigger_a = win_a
        .update(cx, |_root, window, cx| {
            window.focus_next();
            window
                .focused(cx)
                .expect("window A exposes a first tab stop")
        })
        .unwrap();
    let trigger_b = win_b
        .update(cx, |_root, window, cx| {
            window.focus_next();
            window
                .focused(cx)
                .expect("window B exposes a first tab stop")
        })
        .unwrap();
    for (view, _) in [(&view_a, win_a), (&view_b, win_b)] {
        view.update(cx, |view, cx| {
            view.show_settings();
            cx.notify();
        });
    }
    draw(cx, win_a);
    draw(cx, win_b);

    assert!(taskmanager_modal_focused(cx, win_a));
    assert!(taskmanager_modal_focused(cx, win_b));

    // B closes its own dialog; A's must stay open with its scope intact.
    cx.dispatch_keystroke(win_b.into(), Keystroke::parse("escape").unwrap());
    assert!(
        !view_b.read_with(cx, |view, _cx| view.settings_open()),
        "B's Escape must close B's dialog"
    );
    assert!(
        view_a.read_with(cx, |view, _cx| view.settings_open()),
        "B's Escape must not close A's dialog"
    );
    assert!(
        win_b
            .update(cx, |_root, window, _cx| trigger_b.is_focused(window))
            .unwrap(),
        "B's dialog close must restore B's trigger"
    );
    assert!(
        taskmanager_modal_focused(cx, win_a),
        "A's modal scope must survive B's close"
    );

    // A closes its own dialog afterwards, restoring A's own trigger.
    cx.dispatch_keystroke(win_a.into(), Keystroke::parse("escape").unwrap());
    assert!(
        !view_a.read_with(cx, |view, _cx| view.settings_open()),
        "A's Escape must close A's dialog"
    );
    assert!(
        win_a
            .update(cx, |_root, window, _cx| trigger_a.is_focused(window))
            .unwrap(),
        "A's dialog close must restore A's trigger"
    );
}

/// Scenario (b): with a dialog open in A, interacting with B and switching back
/// must leave A's trap intact and restore the exact trigger on close.
#[gpui::test]
async fn dialog_focus_recovers_after_switching_window_and_back(cx: &mut TestAppContext) {
    let (win_a, view_a) = wrapped_root(cx);
    let (win_b, view_b) = wrapped_root(cx);
    prime(cx, &view_a);
    prime(cx, &view_b);
    draw(cx, win_a);
    draw(cx, win_b);

    let trigger_a = win_a
        .update(cx, |_root, window, cx| {
            window.focus_next();
            window
                .focused(cx)
                .expect("window A exposes a first tab stop")
        })
        .unwrap();
    view_a.update(cx, |view, cx| {
        view.show_settings();
        cx.notify();
    });
    draw(cx, win_a);
    assert!(taskmanager_modal_focused(cx, win_a));

    // "Switch away": B takes and moves focus, then the dispatch pipeline runs
    // in B. A's trap and dialog state must be untouched by B's activity.
    win_b
        .update(cx, |_root, window, _cx| window.focus_next())
        .unwrap();
    cx.dispatch_keystroke(win_b.into(), Keystroke::parse("tab").unwrap());
    let b_focus = focused_handle(cx, win_b).expect("B keeps a focused tab stop");
    assert!(
        view_a.read_with(cx, |view, _cx| view.settings_open()),
        "B's interaction must not close A's dialog"
    );
    assert!(taskmanager_modal_focused(cx, win_a));

    // "Switch back": A's Tab still wraps inside its own modal scope.
    for _ in 0..64 {
        cx.dispatch_keystroke(win_a.into(), Keystroke::parse("tab").unwrap());
        assert!(
            taskmanager_modal_focused(cx, win_a),
            "Tab in A must wrap inside A's modal instead of entering the inert page"
        );
    }

    // Closing from A restores the exact pre-dialog trigger in A.
    cx.dispatch_keystroke(win_a.into(), Keystroke::parse("escape").unwrap());
    assert!(
        !view_a.read_with(cx, |view, _cx| view.settings_open()),
        "Escape in A must close A's dialog after the window round-trip"
    );
    assert!(
        win_a
            .update(cx, |_root, window, _cx| trigger_a.is_focused(window))
            .unwrap(),
        "closing A's modal after the round-trip must restore A's exact trigger"
    );
    assert_eq!(
        focused_handle(cx, win_b),
        Some(b_focus),
        "closing A's dialog must not disturb B's focus"
    );
}

/// The Run dialog's command input must be per-window. Typing into window B's
/// Each Run field is its window's sole command-text authority; typing in one
/// window must never mutate the other entity.
#[gpui::test]
async fn run_input_typing_is_isolated_per_window(cx: &mut TestAppContext) {
    let (win_a, view_a) = wrapped_root(cx);
    let (win_b, view_b) = wrapped_root(cx);
    prime(cx, &view_a);
    prime(cx, &view_b);
    draw(cx, win_a);
    draw(cx, win_b);

    // Both windows open their own Run dialog; B opens second (the regression
    // order — B used to render window A's shared input entity).
    for view in [&view_a, &view_b] {
        view.update(cx, |view, cx| {
            view.show_run_task();
            cx.notify();
        });
    }
    draw(cx, win_a);
    draw(cx, win_b);

    // B: focus its own command input (click) and type.
    click_own_text_input(cx, win_b);
    cx.simulate_input(win_b.into(), "echo");
    assert_eq!(
        view_b.read_with(cx, RootView::run_command_text),
        "echo",
        "typing in B must land in B's input authority"
    );
    assert_eq!(
        view_a.read_with(cx, RootView::run_command_text),
        "",
        "B's typing must never write into A's input authority"
    );

    // Reverse: typing in A updates only A; B keeps its own command text.
    click_own_text_input(cx, win_a);
    cx.simulate_input(win_a.into(), "lsla");
    assert_eq!(
        view_a.read_with(cx, RootView::run_command_text),
        "lsla",
        "typing in A must land in A's input authority"
    );
    assert_eq!(
        view_b.read_with(cx, RootView::run_command_text),
        "echo",
        "A's typing must never disturb B's input authority"
    );
}

/// The Apps-page search box must be per-window, same as the Run input: typing
/// into window B's search box updates B's `search_query` only, and window A's
/// query (and the reverse direction) stays untouched.
#[gpui::test]
async fn apps_search_typing_is_isolated_per_window(cx: &mut TestAppContext) {
    let (win_a, view_a) = wrapped_root(cx);
    let (win_b, view_b) = wrapped_root(cx);
    for view in [&view_a, &view_b] {
        view.update(cx, |view, cx| {
            view.mark_telemetry_frame_ready();
            view.page = TopPage::Apps;
            cx.notify();
        });
    }
    draw(cx, win_a);
    draw(cx, win_b);

    // B: focus B's own search box (click) and type.
    click_own_text_input(cx, win_b);
    cx.simulate_input(win_b.into(), "fire");
    settle_search_debounce(cx);
    assert_eq!(
        view_b.read_with(cx, |view, _cx| view.process_query().to_owned()),
        "fire",
        "typing in B's search box must update B's search_query"
    );
    assert_eq!(
        view_a.read_with(cx, |view, _cx| view.process_query().to_owned()),
        "",
        "B's search typing must never write into A's search_query"
    );

    // Reverse: A's search box updates only A; B keeps its own query.
    click_own_text_input(cx, win_a);
    cx.simulate_input(win_a.into(), "rust");
    settle_search_debounce(cx);
    assert_eq!(
        view_a.read_with(cx, |view, _cx| view.process_query().to_owned()),
        "rust",
        "typing in A's search box must update A's search_query"
    );
    assert_eq!(
        view_b.read_with(cx, |view, _cx| view.process_query().to_owned()),
        "fire",
        "A's search typing must never disturb B's search_query"
    );
}

/// The Services/Startup search boxes were the last `thread_local`-held inputs:
/// they crossed window boundaries and re-entered `root.update` from the Change
/// subscription (panicking on real input). Both are now per-window on the
/// RootView; typing into B's Services search must update B's query only.
#[gpui::test]
async fn services_search_typing_is_isolated_per_window(cx: &mut TestAppContext) {
    let (win_a, view_a) = wrapped_root(cx);
    let (win_b, view_b) = wrapped_root(cx);
    for view in [&view_a, &view_b] {
        view.update(cx, |view, cx| {
            view.mark_telemetry_frame_ready();
            view.page = TopPage::Services;
            cx.notify();
        });
    }
    draw(cx, win_a);
    draw(cx, win_b);

    click_own_text_input(cx, win_b);
    cx.simulate_input(win_b.into(), "networkd");
    settle_search_debounce(cx);
    assert_eq!(
        view_b.read_with(cx, |view, _cx| view.services_state.query.clone()),
        "networkd",
        "typing in B's services search must update B's query"
    );
    assert_eq!(
        view_a.read_with(cx, |view, _cx| view.services_state.query.clone()),
        "",
        "B's typing must never write into A's services query"
    );

    click_own_text_input(cx, win_a);
    cx.simulate_input(win_a.into(), "cron");
    settle_search_debounce(cx);
    assert_eq!(
        view_a.read_with(cx, |view, _cx| view.services_state.query.clone()),
        "cron",
        "typing in A's services search must update A's query"
    );
    assert_eq!(
        view_b.read_with(cx, |view, _cx| view.services_state.query.clone()),
        "networkd",
        "A's typing must never disturb B's services query"
    );
}

// ── Settings dialog per-window state ─────────────────────────────────────────
//
// The Settings dialog used to hold three `thread_local`s that crossed window
// boundaries: the text-rendering token, the startup-page token, and the
// refresh-interval `Entity<SliderState>`. All three now live on each window's
// own RootView (`settings_text_rendering` / `settings_startup_page` /
// `settings_slider`), following the services/startup per-window pattern.

/// The "Choose columns" dropdown (own `DropdownMenu`) must toggle column
/// visibility through the real menu: click the action-strip trigger, click
/// the second row (User — the first row Name is the fixed identity column),
/// and `hidden_cols` gains the column; clicking again removes it. The menu
/// dismisses after each choice.
#[gpui::test]
async fn choose_columns_menu_toggles_hidden_cols(cx: &mut TestAppContext) {
    use taskmanager_shell::SortCol;

    let (win, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Apps;
        cx.notify();
    });
    draw(cx, win);

    let pick = |cx: &mut TestAppContext| {
        let mut vcx = VisualTestContext::from_window(win.into(), cx);
        let trigger = vcx
            .debug_bounds("columns-trigger")
            .expect("the action strip renders the columns trigger");
        vcx.simulate_mouse_move(
            trigger.center(),
            None::<gpui::MouseButton>,
            Default::default(),
        );
        vcx.simulate_click(trigger.center(), Default::default());
        drop(vcx);
        draw(cx, win);
        // The opening frame renders at the previous anchor; re-draw settles it.
        draw(cx, win);
        let mut vcx = VisualTestContext::from_window(win.into(), cx);
        let popup = vcx
            .debug_bounds("tm-popup")
            .expect("the trigger click opens the columns menu");
        // Rows are 26px tall inside the body's 4px top padding; the second
        // row is User (Name is the inert first row).
        let row = point(popup.left() + px(40.0), popup.top() + px(4.0 + 26.0 + 13.0));
        vcx.simulate_mouse_move(row, None::<gpui::MouseButton>, Default::default());
        vcx.simulate_click(row, Default::default());
        drop(vcx);
        draw(cx, win);
    };

    // Hide the User column.
    pick(cx);
    assert!(
        view.read_with(cx, |v, _| v
            .processes_state
            .hidden_cols
            .contains(&SortCol::User)),
        "choosing the User row must hide the User column"
    );

    // The menu dismissed after the choice; reopening and choosing User again
    // restores it.
    pick(cx);
    assert!(
        !view.read_with(cx, |v, _| v
            .processes_state
            .hidden_cols
            .contains(&SortCol::User)),
        "choosing the User row again must restore the User column"
    );
    assert!(
        !view.read_with(cx, |v, _| v
            .processes_state
            .hidden_cols
            .contains(&SortCol::Name)),
        "Name (the identity column) must never be hidden by the menu"
    );
}
