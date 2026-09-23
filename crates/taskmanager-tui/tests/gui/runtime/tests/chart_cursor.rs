//! Keyboard chart-cursor tests: the terminal port of the Iced chart hover.
//!
//! `←`/`→` on the Performance·CPU history chart move a sample cursor and the
//! chart paints the same per-sample readout a pointer hover would produce. The
//! tests drive `handle_key` (the same path crossterm uses) and assert on the
//! drawn frame text, never source `.contains()`.

use super::super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyCode;

use taskmanager_application::AppAction;

use crate::TuiTheme;
use crate::render;

/// Render the live frame through the same TestBackend path the render tests
/// use.
fn frame_text(app: &crate::TuiApp, width: u16, height: u16) -> String {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render(frame, app, TuiTheme::default()))
        .expect("draw");
    terminal.backend().to_string()
}

fn app_on_cpu() -> crate::TuiApp {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Performance));
    app.perf_device = crate::PerfDevice::Cpu;
    app
}

/// The cursor opens on the newest sample and steps left one sample per press,
/// clamped at the oldest; the painted readout follows the cursor and equals the
/// value a hover at that index would report.
#[test]
fn arrows_move_the_chart_cursor_and_paint_the_hover_readout() {
    let mut app = app_on_cpu();
    let samples = taskmanager_shell::presentation::trend::cpu_usage_percent(&app.history);
    assert!(
        samples.len() >= 2,
        "the demo history seeds a steppable CPU series"
    );
    let last = samples.len() - 1;
    let expected = |index: usize| {
        format!(
            "CPU · {}/{} · {:.0}%",
            index + 1,
            samples.len(),
            samples[index]
        )
    };

    assert!(
        app.chart_cursor.is_none(),
        "no cursor before the first arrow press"
    );

    // Right opens the cursor on the newest sample and paints its readout.
    let _ = handle_key(&mut app, KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    assert_eq!(app.chart_cursor, Some(last));
    let text = frame_text(&app, 140, 40);
    assert!(
        text.contains(&expected(last)),
        "the newest-sample readout must paint:\n{text}"
    );

    // Left steps to the previous sample; the readout follows the cursor.
    let _ = handle_key(&mut app, KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    assert_eq!(app.chart_cursor, Some(last - 1));
    let text = frame_text(&app, 140, 40);
    assert!(
        text.contains(&expected(last - 1)),
        "the stepped readout must paint:\n{text}"
    );

    // Left clamps at the oldest sample instead of wrapping or panicking.
    for _ in 0..samples.len() + 4 {
        let _ = handle_key(&mut app, KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    }
    assert_eq!(app.chart_cursor, Some(0));
    let text = frame_text(&app, 140, 40);
    assert!(
        text.contains(&expected(0)),
        "the oldest-sample readout must paint:\n{text}"
    );
}

/// The cursor is Performance·CPU scoped and clears when the device changes, so
/// an arrow on another device (or another page) never moves it.
#[test]
fn the_chart_cursor_is_scoped_to_the_cpu_device() {
    let mut app = app_on_cpu();
    let _ = handle_key(&mut app, KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    assert!(app.chart_cursor.is_some());

    // Selecting another Performance resource clears the cursor and the arrows
    // are inert there.
    app.select_perf_device(crate::PerfDevice::Memory);
    assert!(app.chart_cursor.is_none());
    let _ = handle_key(&mut app, KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    assert!(
        app.chart_cursor.is_none(),
        "the chart cursor is CPU-device scoped"
    );

    // Off the Performance page the same arrows change nothing either.
    let mut processes = app_on_cpu();
    let _ = processes.apply_action(AppAction::SelectPage(AppPage::Applications));
    let _ = handle_key(
        &mut processes,
        KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
    );
    assert!(processes.chart_cursor.is_none());
}

/// A window too short to hover has no cursor and a gap sample keeps the shared
/// dash instead of a fabricated number — the same honest-absence rule the
/// pointer hover obeys.
#[test]
fn a_short_window_has_no_cursor_and_a_gap_keeps_the_dash() {
    use crate::ui::chart_cursor::{move_cursor, readout_line};

    assert_eq!(move_cursor(None, 1, 0), None, "empty window");
    assert_eq!(move_cursor(None, -1, 1), None, "single sample");
    assert_eq!(
        move_cursor(Some(0), 1, 1),
        None,
        "single sample with cursor"
    );

    let gap = readout_line("CPU", &[f32::NAN, 50.0], 0, |value| format!("{value:.0}%"));
    assert_eq!(
        gap,
        Some(format!(
            "CPU · 1/2 · {}",
            taskmanager_shell::presentation::missing_value()
        )),
        "a gap sample keeps the shared dash"
    );
    assert_eq!(
        readout_line("CPU", &[10.0, 50.0], 5, |value| format!("{value:.0}%")),
        None,
        "an out-of-range index has no readout"
    );
}
