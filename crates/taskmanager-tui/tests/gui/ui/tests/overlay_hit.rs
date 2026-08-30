//! Overlay HitMap behavior tests (TUI-001): the actionable controls painted
//! inside an overlay popup — the six menu surfaces' rows and the command
//! palette's filtered command rows — are typed `OverlayControl` hits that the
//! pointer seam consumes through the SAME mutators and Enter methods the
//! keyboard uses. Every other popup cell stays a blocked `Overlay` no-op, and
//! a stale committed plan can never fall through to the background table.
//!
//! Alignment evidence is paint-derived: the highlight-marker tests render a
//! full frame through `render_with_plan` with the exact plan under test and
//! resolve the painted marker cell back through `hit_target`, so the drawn
//! row and the hit row are provably the same projection.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use taskmanager_application::{AppAction, AppPage};
use taskmanager_core::core::process::ProcessLiveKey;

use crate::ui::{TuiFramePlan, TuiHitTarget};

const FRAME: Rect = Rect::new(0, 0, 120, 40);

fn click(column: u16, row: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

/// Render the full frame for `app` through the exact plan a pointer click
/// would consume, and return the painted text next to that plan. Pins
/// English so glyph/text scanning cannot depend on the host locale.
fn painted_frame(app: &crate::TuiApp) -> (String, TuiFramePlan) {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let plan = TuiFramePlan::build(app, FRAME);
    let backend = TestBackend::new(FRAME.width, FRAME.height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| crate::ui::render_with_plan(frame, app, crate::TuiTheme::default(), &plan))
        .expect("draw");
    (terminal.backend().to_string(), plan)
}

/// The absolute y of the row inside `popup` that paints control `index` for
/// the four action menus whose renderer paints a frozen-target line and a
/// blank line above the first action row, below one border row. These
/// constants are the independent geometry evidence: if the plan's projection
/// drifts from the renderer, the click lands elsewhere and the effect
/// assertion below fails.
fn menu_row(popup: Rect, index: u16) -> u16 {
    popup.y + 1 + 2 + index
}

#[test]
fn the_painted_menu_highlight_resolves_through_the_committed_plan() {
    // Paint/hit same-source: the committed plan rendered the frame, and the
    // cell the renderer highlighted with the ▸ marker must resolve back to
    // that row's OverlayControl through the same plan.
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Services));
    assert!(
        app.open_service_menu(),
        "demo fixture exposes a service menu"
    );
    let (text, plan) = painted_frame(&app);
    let popup = plan.overlay().expect("service menu owns a popup").popup;
    assert!(
        popup.x > 0 && popup.y > 0,
        "the centered popup must sit inside the frame"
    );

    let marker_row = |text: &str, popup: Rect| {
        (popup.y..popup.y.saturating_add(popup.height))
            .find(|row| {
                text.lines()
                    .nth(usize::from(*row))
                    .is_some_and(|line| line.contains('▸'))
            })
            .unwrap_or_else(|| panic!("the open menu must paint its ▸ highlight inside the popup"))
    };

    // The menu opens at selection 0, and the painted marker's first inner
    // cell resolves to exactly that control.
    let highlighted = marker_row(&text, popup);
    assert_eq!(
        plan.hit_target(popup.x + 1, highlighted),
        Some(TuiHitTarget::OverlayControl {
            surface: crate::TuiSurfaceKind::ServiceMenu,
            index: 0,
        }),
        "the painted highlight cell must hit the painted row's control"
    );

    // Move the cursor like the keyboard does, repaint, and the same
    // projection must now address control 1 — the highlight and the hit map
    // move together because they share one plan.
    app.service_menu_move(1);
    let (text, plan) = painted_frame(&app);
    let popup = plan.overlay().expect("service menu popup").popup;
    assert_eq!(
        plan.hit_target(popup.x + 1, marker_row(&text, popup)),
        Some(TuiHitTarget::OverlayControl {
            surface: crate::TuiSurfaceKind::ServiceMenu,
            index: 1,
        })
    );

    // Non-control popup cells (the frozen-target header line) stay blocked.
    assert_eq!(
        plan.hit_target(popup.x + 1, popup.y + 1),
        Some(TuiHitTarget::Overlay {
            scope: plan.overlay().expect("overlay").scope,
        }),
        "the menu's header line is a blocked overlay cell, not a control"
    );
}

#[test]
fn service_menu_row_click_walks_the_keyboard_confirmation_gate() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Services));
    assert!(app.open_service_menu());
    let plan = TuiFramePlan::build(&app, FRAME);
    let popup = plan.overlay().expect("service menu popup").popup;

    // Click the second action row ("Stop").
    let reaction = crate::runtime::runtime_support::apply_terminal_event_with_plan(
        &mut app,
        click(popup.x + 5, menu_row(popup, 1)),
        &plan,
    );
    assert!(reaction.dirty, "a landed overlay click repaints");
    assert!(
        reaction.effect.is_none(),
        "the menu click must not emit the platform request itself"
    );
    // Enter parity: the pick armed the shared gated confirmation with the
    // chosen action; only the keyboard's y submits the effect.
    let pending = app
        .shell
        .pending_service_control()
        .expect("the click must arm the same gated confirmation as Enter");
    assert_eq!(
        pending.action,
        taskmanager_core::core::services::ServiceAction::Stop,
        "the clicked row's action is what the gate froze"
    );
}

#[test]
fn process_menu_click_routes_control_and_gated_rows_like_enter() {
    // The direct batch row submits through the same path the keyboard's Enter
    // uses; the gated row arms the confirmation instead of emitting.
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    assert!(
        app.open_process_menu(),
        "demo fixture exposes a process menu"
    );
    let plan = TuiFramePlan::build(&app, FRAME);
    let popup = plan.overlay().expect("process menu popup").popup;

    // Click "Suspend" (direct batch row).
    let reaction = crate::runtime::runtime_support::apply_terminal_event_with_plan(
        &mut app,
        click(popup.x + 5, menu_row(popup, 3)),
        &plan,
    );
    assert!(reaction.dirty);
    assert!(
        reaction.effect.is_some(),
        "the direct batch row submits like keyboard Enter"
    );
    assert!(
        app.shell.pending_confirmation().is_none(),
        "a non-destructive row must not open a confirmation"
    );

    // Fresh app: click "End task" (the gated first row).
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    assert!(app.open_process_menu());
    let plan = TuiFramePlan::build(&app, FRAME);
    let popup = plan.overlay().expect("process menu popup").popup;
    let reaction = crate::runtime::runtime_support::apply_terminal_event_with_plan(
        &mut app,
        click(popup.x + 5, menu_row(popup, 0)),
        &plan,
    );
    assert!(reaction.dirty);
    assert!(
        reaction.effect.is_none(),
        "the gated row must not emit the end-task effect"
    );
    assert!(
        matches!(
            app.shell.pending_confirmation(),
            Some(taskmanager_application::PendingConfirmation::EndTask(_))
        ),
        "the gated row must arm the identity-frozen end-task gate"
    );
}

#[test]
fn session_menu_row_click_arms_the_session_gate() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Users));
    assert!(
        app.open_session_menu(),
        "demo fixture exposes a session menu"
    );
    let plan = TuiFramePlan::build(&app, FRAME);
    let popup = plan.overlay().expect("session menu popup").popup;

    // Click "Lock" (the second action row).
    let reaction = crate::runtime::runtime_support::apply_terminal_event_with_plan(
        &mut app,
        click(popup.x + 5, menu_row(popup, 1)),
        &plan,
    );
    assert!(reaction.dirty);
    let pending = app
        .shell
        .pending_session()
        .expect("the click must arm the shared session gate");
    assert_eq!(
        pending.action,
        taskmanager_core::core::session::SessionControlAction::Lock
    );
    assert!(
        reaction.effect.is_none(),
        "the session request is only emitted by the y confirmation"
    );
}

#[test]
fn batch_menu_row_click_routes_through_the_shared_batch_path() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    // One marked process is enough to open the batch menu (the count is
    // frozen at open time; the shell owns the live set).
    app.shell.selected_rows.insert(
        ProcessLiveKey::from_parts(4242, taskmanager_test_support::fixture_start_token(4242))
            .expect("non-zero parts"),
    );
    assert!(app.open_batch_menu(), "a marked set opens the batch menu");
    let plan = TuiFramePlan::build(&app, FRAME);
    let popup = plan.overlay().expect("batch menu popup").popup;

    // Click "Clear selection" (the last action row).
    let reaction = crate::runtime::runtime_support::apply_terminal_event_with_plan(
        &mut app,
        click(popup.x + 5, menu_row(popup, 7)),
        &plan,
    );
    assert!(reaction.dirty);
    assert!(
        app.shell.selected_rows.is_empty(),
        "the clicked Clear row must empty the marked set exactly like Enter"
    );
    assert!(
        app.local_surface_kind().is_none(),
        "the batch menu is consumed by the selection"
    );
}

#[test]
fn column_menu_row_click_toggles_that_column() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    app.toggle_column_menu();
    let plan = TuiFramePlan::build(&app, FRAME);
    let popup = plan.overlay().expect("column menu popup").popup;

    // The column menu paints its rows directly at the body top (no frozen
    // target line): border row + index. Click row 0 (the CPU column).
    let reaction = crate::runtime::runtime_support::apply_terminal_event_with_plan(
        &mut app,
        click(popup.x + 5, popup.y + 1),
        &plan,
    );
    assert!(reaction.dirty);
    assert!(
        !app.column_visible(taskmanager_shell::SortCol::Cpu),
        "clicking the CPU row must hide it, like Enter/Space on the cursor"
    );
    assert_eq!(
        app.local_surface_kind(),
        Some(crate::TuiSurfaceKind::ColumnMenu),
        "the column menu stays open after a toggle"
    );
}

#[test]
fn command_palette_row_click_runs_the_selected_command() {
    // The filter narrows through the (English-pinned) labels, so pin the
    // language like every label-dependent behavior test.
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);

    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    app.open_command_palette();
    for character in "serv".chars() {
        app.palette_push_char(character);
    }
    let first = app
        .filtered_palette_rows()
        .first()
        .copied()
        .expect("the filter keeps the Services row");
    assert!(
        first.action.is_some(),
        "the first filtered row must be the executable ShowServices command"
    );

    let plan = TuiFramePlan::build(&app, FRAME);
    let popup = plan.overlay().expect("palette popup").popup;
    // The palette paints a 3-row filter field above the first command row.
    let reaction = crate::runtime::runtime_support::apply_terminal_event_with_plan(
        &mut app,
        click(popup.x + 10, popup.y + 1 + 3),
        &plan,
    );
    assert!(reaction.dirty);
    assert_eq!(
        app.page(),
        AppPage::Services,
        "clicking the row must run it exactly like palette Enter"
    );
    assert!(
        app.command_palette().is_none(),
        "the palette closes after running the clicked command"
    );
}

#[test]
fn a_stale_overlay_plan_fail_closed_after_a_surface_switch() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Services));
    assert!(app.open_service_menu());
    let plan = TuiFramePlan::build(&app, FRAME);
    let popup = plan.overlay().expect("service menu popup").popup;
    let selected_before = app.shell.selected;

    // Switch surfaces WITHOUT repainting: the menu closes and the palette
    // opens, so the committed plan now describes a popup that is not on
    // screen anymore.
    app.close_local_overlays();
    app.open_command_palette();

    // The old plan still names the menu's control row...
    assert_eq!(
        plan.hit_target(popup.x + 5, menu_row(popup, 1)),
        Some(TuiHitTarget::OverlayControl {
            surface: crate::TuiSurfaceKind::ServiceMenu,
            index: 1,
        })
    );
    // ...but applying the click must be a no-op: the palette owns the
    // keyboard, so the click may neither arm the dead menu nor run a palette
    // command through geometry that never painted the palette.
    let reaction = crate::runtime::runtime_support::apply_terminal_event_with_plan(
        &mut app,
        click(popup.x + 5, menu_row(popup, 1)),
        &plan,
    );
    assert_eq!(
        reaction,
        crate::runtime::runtime_support::EventReaction::default()
    );
    assert!(app.shell.pending_service_control().is_none());
    let palette = app
        .command_palette()
        .expect("the stale click must not close the palette");
    assert_eq!(palette.selection, 0, "the stale click must not move it");

    // Outside the stale popup the old plan still projects a Services table
    // row, but a surface owns the keyboard, so the row path is blocked too —
    // the click never lands on the background table.
    let panel = plan.table_panel().expect("the stale Services plan");
    let background = (panel.area.x + 2, panel.area.y + 3);
    assert!(matches!(
        plan.hit_target(background.0, background.1),
        Some(TuiHitTarget::TableRow { .. })
    ));
    let reaction = crate::runtime::runtime_support::apply_terminal_event_with_plan(
        &mut app,
        click(background.0, background.1),
        &plan,
    );
    assert_eq!(
        reaction,
        crate::runtime::runtime_support::EventReaction::default()
    );
    assert_eq!(app.shell.selected, selected_before);
}

#[test]
fn confirmation_and_viewport_popups_stay_blocked_no_ops() {
    // The identity-frozen end-task confirmation paints Confirm/Cancel text
    // but NO pointer controls: the destructive answer stays on the keyboard's
    // y/n, and every popup cell is a blocked no-op.
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    let _ = app.apply_action(AppAction::RequestEndTask);
    assert!(
        app.shell.pending_confirmation().is_some(),
        "the demo selection must arm the end-task gate"
    );
    let plan = TuiFramePlan::build(&app, FRAME);
    let overlay = plan.overlay().expect("confirmation owns a popup");
    assert_eq!(
        overlay.controls, None,
        "the confirmation must model no pointer controls"
    );
    let center = (
        overlay.popup.x + overlay.popup.width / 2,
        overlay.popup.y + overlay.popup.height / 2,
    );
    assert_eq!(
        plan.hit_target(center.0, center.1),
        Some(TuiHitTarget::Overlay {
            scope: overlay.scope,
        })
    );
    let reaction = crate::runtime::runtime_support::apply_terminal_event_with_plan(
        &mut app,
        click(center.0, center.1),
        &plan,
    );
    assert_eq!(
        reaction,
        crate::runtime::runtime_support::EventReaction::default()
    );
    assert!(
        matches!(
            app.shell.pending_confirmation(),
            Some(taskmanager_application::PendingConfirmation::EndTask(_))
        ),
        "the click must neither submit nor dismiss the gate"
    );

    // The help viewport is likewise read-only: every popup cell is blocked
    // and a click changes nothing.
    app.shell.dismiss_overlay();
    app.toggle_help();
    let plan = TuiFramePlan::build(&app, FRAME);
    let overlay = plan.overlay().expect("help owns a popup");
    assert_eq!(
        overlay.controls, None,
        "the help viewport models no controls"
    );
    let center = (
        overlay.popup.x + overlay.popup.width / 2,
        overlay.popup.y + overlay.popup.height / 2,
    );
    assert_eq!(
        plan.hit_target(center.0, center.1),
        Some(TuiHitTarget::Overlay {
            scope: overlay.scope,
        })
    );
    let reaction = crate::runtime::runtime_support::apply_terminal_event_with_plan(
        &mut app,
        click(center.0, center.1),
        &plan,
    );
    assert_eq!(
        reaction,
        crate::runtime::runtime_support::EventReaction::default()
    );
    assert!(app.shell.help_open(), "the help overlay stays open");
}
