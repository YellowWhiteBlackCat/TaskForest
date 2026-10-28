//! CORE-08 behavior anchors for the TUI capability declaration.
//!
//! These tests drive the production `handle_key` router and assert the live
//! model plus TestBackend frame. They deliberately do not construct component
//! values or inject shell `Message` variants.

use super::super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use std::time::Duration;
use taskmanager_application::i18n::{Language, set_language};
use taskmanager_application::{AppAction, AppPage};
use taskmanager_core::core::process::FrozenProcessIdentity;
use taskmanager_shell::SortCol;

use crate::{TuiApp, TuiTheme, render};

fn frame_text(app: &TuiApp, width: u16, height: u16) -> String {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    set_language(Language::En);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render(frame, app, TuiTheme::default()))
        .expect("draw");
    terminal.backend().to_string()
}

fn press(app: &mut TuiApp, code: KeyCode, modifiers: KeyModifiers) -> Option<PlatformEffect> {
    handle_key(app, KeyEvent::new(code, modifiers))
}

fn type_text(app: &mut TuiApp, value: &str) {
    for character in value.chars() {
        let _ = press(app, KeyCode::Char(character), KeyModifiers::NONE);
    }
}

fn app_on_processes() -> TuiApp {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    app
}

#[test]
fn search_filters_the_live_projection_and_escape_restores_it() {
    let mut app = app_on_processes();
    let unfiltered_count = app.visible_processes().len();
    assert!(unfiltered_count > 1);
    let _ = move_to_process_row(&mut app);

    let _ = press(&mut app, KeyCode::Char('f'), KeyModifiers::CONTROL);
    assert!(app.search_active(), "Ctrl+F gives the search input focus");

    type_text(&mut app, "dbus-broker");

    assert_eq!(app.query, "dbus-broker");
    let filtered = app.visible_processes();
    assert_eq!(filtered.len(), 1, "the shell projection applies the query");
    assert_eq!(filtered[0].name, "dbus-broker");
    let filtered_frame = frame_text(&app, 120, 40);
    assert!(filtered_frame.contains("dbus-broker"), "{filtered_frame}");
    drop(filtered);

    let selected = app
        .selected_detail_process()
        .as_ref()
        .and_then(FrozenProcessIdentity::from_process)
        .expect("the filtered visible row has a current identity");
    assert_eq!(selected.name, "dbus-broker");
    let _ = press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(app.query, "", "Escape clears the current query first");
    assert_eq!(
        app.visible_processes().len(),
        unfiltered_count,
        "clearing the query restores the unfiltered process projection"
    );
    assert_eq!(
        app.selected_detail_process()
            .as_ref()
            .and_then(FrozenProcessIdentity::from_process),
        Some(selected.clone()),
        "the same visible row identity remains anchored while the filter clears"
    );
    assert_eq!(app.shell.selected_process_identity(), Some(selected));
    assert!(
        app.search_active(),
        "the first Escape keeps the search field open"
    );

    let _ = press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(
        !app.search_active(),
        "a second Escape leaves the search field"
    );
}

#[test]
fn search_backspace_updates_the_filtered_projection() {
    let mut app = app_on_processes();
    let unfiltered_count = app.visible_processes().len();
    let _ = press(&mut app, KeyCode::Char('f'), KeyModifiers::CONTROL);
    type_text(&mut app, "no-such-process-7f2c");
    assert!(app.visible_processes().is_empty());

    // Backspace is the supported end-edit operation and must reconcile the
    // row cursor against the new filtered projection.
    for _ in 0.."no-such-process-7f2c".chars().count() {
        let _ = press(&mut app, KeyCode::Backspace, KeyModifiers::NONE);
    }
    assert_eq!(app.query, "");
    assert_eq!(app.visible_processes().len(), unfiltered_count);
    assert!(app.selected_detail_process().is_some());
}

#[test]
fn settings_switch_uses_space_and_writes_the_value_to_config() {
    let mut app = crate::demo_app();
    let _ = press(&mut app, KeyCode::Char('p'), KeyModifiers::NONE);
    assert!(app.settings_open());

    for _ in 0..2 {
        let _ = press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    }
    assert_eq!(app.settings_form.field, 2);
    assert!(!app.settings_form.hc);

    let _ = press(&mut app, KeyCode::Char(' '), KeyModifiers::NONE);
    assert!(app.settings_form.hc, "Space toggles the focused switch");
    let frame = frame_text(&app, 100, 36);
    let high_contrast_line = frame
        .lines()
        .find(|line| line.contains("High contrast"))
        .expect("the focused switch renders");
    assert!(high_contrast_line.contains("On"), "{high_contrast_line}");

    let _ = press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(!app.settings_open(), "Enter applies and closes Settings");
    assert!(
        app.config_draft.hc,
        "the Settings consumer receives the toggle"
    );
}

#[test]
fn settings_slider_steps_clamp_and_reach_the_shared_consumers() {
    let mut app = crate::demo_app();
    let _ = press(&mut app, KeyCode::Char('p'), KeyModifiers::NONE);
    for _ in 0..25 {
        let _ = press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    }
    assert_eq!(app.settings_form.field, 25);

    for _ in 0..5 {
        let _ = press(&mut app, KeyCode::Left, KeyModifiers::NONE);
    }
    assert_eq!(app.settings_form.graph_points(), 60, "minimum clamps");
    for _ in 0..5 {
        let _ = press(&mut app, KeyCode::Right, KeyModifiers::NONE);
    }
    assert_eq!(app.settings_form.graph_points(), 600, "maximum clamps");
    for _ in 0..2 {
        let _ = press(&mut app, KeyCode::Left, KeyModifiers::NONE);
    }
    assert_eq!(
        app.settings_form.graph_points(),
        120,
        "Data points steps through the declared [60, 120, 300, 600] choices"
    );

    let frame = frame_text(&app, 100, 36);
    let graph_line = frame
        .lines()
        .find(|line| line.contains("Data points"))
        .expect("the focused Slider renders");
    assert!(graph_line.contains("120"), "{graph_line}");

    let _ = press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(app.config_draft.graph_data_points, 120);
    assert_eq!(app.prefs.graph_points, 120);
    assert_eq!(
        app.shell.history.capacity(),
        120,
        "the save updates the shared graph-history consumer"
    );
}

#[test]
fn settings_quiet_hour_slider_clamps_at_both_ends() {
    let mut app = crate::demo_app();
    let _ = press(&mut app, KeyCode::Char('p'), KeyModifiers::NONE);
    for _ in 0..27 {
        let _ = press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    }
    assert_eq!(app.settings_form.field, 27);

    let _ = press(&mut app, KeyCode::Left, KeyModifiers::NONE);
    assert_eq!(app.settings_form.quiet_start, 0);
    for _ in 0..30 {
        let _ = press(&mut app, KeyCode::Right, KeyModifiers::NONE);
    }
    assert_eq!(app.settings_form.quiet_start, 23);

    let frame = frame_text(&app, 100, 36);
    let quiet_hours_line = frame
        .lines()
        .find(|line| line.contains("Quiet hours start"))
        .expect("the focused Slider renders");
    assert!(quiet_hours_line.contains("23:00"), "{quiet_hours_line}");
}

#[test]
fn dropdown_menu_keeps_multiple_column_toggles_open() {
    let mut app = app_on_processes();
    assert!(app.column_visible(SortCol::Cpu));
    assert!(app.column_visible(SortCol::Memory));

    let _ = press(&mut app, KeyCode::Char('C'), KeyModifiers::SHIFT);
    assert!(app.column_menu_open(), "Shift+C opens the keyboard menu");

    let _ = press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    let _ = press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(!app.column_visible(SortCol::Memory));
    assert!(
        app.column_menu_open(),
        "the first toggle leaves the menu open"
    );

    let _ = press(&mut app, KeyCode::Up, KeyModifiers::NONE);
    let _ = press(&mut app, KeyCode::Char(' '), KeyModifiers::NONE);
    assert!(!app.column_visible(SortCol::Cpu));
    assert!(
        app.column_menu_open(),
        "a second item can be toggled in place"
    );

    let frame = frame_text(&app, 120, 40);
    assert!(
        frame.contains("Columns"),
        "the menu remains visible: {frame}"
    );
    assert!(!app.column_visible(SortCol::Cpu));
    let cpu_menu_row = frame
        .lines()
        .find(|line| {
            line.split_once('›').is_some_and(|(_, tail)| {
                let label = tail.trim_start();
                label.starts_with("CPU") && !label.starts_with("CPU time")
            })
        })
        .expect("the selected CPU visibility row is painted");
    assert!(
        !cpu_menu_row.contains('✓'),
        "the CPU visibility row must not show a checkmark: {cpu_menu_row}"
    );
    assert!(app.hidden_columns.contains(&SortCol::Cpu));
    assert!(app.hidden_columns.contains(&SortCol::Memory));

    let _ = press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(!app.column_menu_open(), "Escape cancels the menu");
}

fn move_to_process_row(app: &mut TuiApp) -> FrozenProcessIdentity {
    for _ in 0..app.visual_row_count() {
        if let Some(identity) = app
            .selected_detail_process()
            .as_ref()
            .and_then(FrozenProcessIdentity::from_process)
        {
            return identity;
        }
        let _ = press(app, KeyCode::Down, KeyModifiers::NONE);
    }
    panic!("the demo Applications tree must expose an actionable process row");
}

#[test]
fn table_sort_preserves_the_current_row_and_opens_its_action_menu() {
    let mut app = app_on_processes();
    let identity = move_to_process_row(&mut app);
    let selected_name = app
        .selected_detail_process()
        .expect("selected process row")
        .name;

    let _ = press(&mut app, KeyCode::Char('s'), KeyModifiers::NONE);
    assert_eq!(app.effective_sort_col(), SortCol::Memory);
    assert_eq!(
        app.shell.selected_process_identity(),
        Some(identity),
        "keyboard sort preserves the selected row's frozen identity"
    );
    let sorted_frame = frame_text(&app, 120, 40);
    assert!(sorted_frame.contains(&selected_name), "{sorted_frame}");
    assert!(
        sorted_frame.contains("Memory"),
        "the active sort is painted: {sorted_frame}"
    );

    let _ = press(&mut app, KeyCode::Char('a'), KeyModifiers::NONE);
    assert_eq!(
        app.local_surface_kind(),
        Some(crate::TuiSurfaceKind::ProcessMenu),
        "the current process has an action menu"
    );
    let frame = frame_text(&app, 120, 40);
    assert!(
        frame.contains(&selected_name),
        "the menu names its frozen row: {frame}"
    );
    assert!(
        frame.contains("Search online"),
        "an available row action renders: {frame}"
    );

    // The action returns through the production key router. The test does not
    // submit the platform effect, so no external browser or OS action occurs.
    let effect = press(&mut app, KeyCode::Char('w'), KeyModifiers::NONE);
    assert!(
        effect.is_some(),
        "the keyboard row action produces its effect"
    );
    assert_ne!(
        app.local_surface_kind(),
        Some(crate::TuiSurfaceKind::ProcessMenu)
    );
}

#[test]
fn modal_focus_traps_table_keys_and_escape_returns_focus() {
    let mut app = app_on_processes();
    let identity = move_to_process_row(&mut app);
    let _ = press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(
        app.process_properties().is_some(),
        "Enter opens row properties"
    );
    assert_eq!(app.shell.selected_process_identity(), Some(identity));

    let selected = app.selected;
    let _ = press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    assert_eq!(app.selected, selected, "the modal traps the table cursor");
    assert!(app.process_properties().is_some());

    // Popup chrome is a blocked overlay cell; an outside press is the scrim
    // gesture the shared modal policy maps onto the same close path.
    let plan = crate::ui::TuiFramePlan::build(&app, Rect::new(0, 0, 120, 40));
    let popup = plan.overlay().expect("properties overlay").popup;
    assert!(plan.hit_target(0, 0).is_none());
    let chrome = crate::runtime::runtime_support::apply_terminal_event_with_plan(
        &mut app,
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: popup.x,
            row: popup.y,
            modifiers: KeyModifiers::NONE,
        }),
        &plan,
    );
    assert_eq!(
        chrome,
        crate::runtime::runtime_support::EventReaction::default(),
        "popup chrome stays a blocked no-op"
    );
    assert!(app.process_properties().is_some());

    let reaction = crate::runtime::runtime_support::apply_terminal_event_with_plan(
        &mut app,
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        }),
        &plan,
    );
    assert!(reaction.dirty, "the scrim press repaints the closed modal");
    assert!(
        app.process_properties().is_none(),
        "an outside press cancels the modal"
    );
    let _ = press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    assert_ne!(app.selected, selected, "the table regains keyboard focus");
}

#[test]
fn refresh_slider_writeback_reaches_the_telemetry_policy() {
    let mut app = crate::demo_app();
    let _ = press(&mut app, KeyCode::Char('p'), KeyModifiers::NONE);
    for _ in 0..7 {
        let _ = press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    }
    assert_eq!(app.settings_form.field, 7);
    let _ = press(&mut app, KeyCode::Right, KeyModifiers::NONE);
    assert_eq!(app.settings_form.refresh_ms(), 2_000);
    let frame = frame_text(&app, 100, 36);
    let refresh_line = frame
        .lines()
        .find(|line| line.contains("Refresh interval"))
        .expect("the focused Slider renders");
    assert!(refresh_line.contains("2 s"), "{refresh_line}");

    let _ = press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(app.config_draft.refresh_ms, 2_000);
    assert_eq!(
        app.shell.telemetry_interval().duration(),
        Duration::from_secs(2)
    );
}
