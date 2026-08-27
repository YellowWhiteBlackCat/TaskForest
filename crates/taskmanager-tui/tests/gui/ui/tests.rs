//! TUI render unit tests, split by topic.
//!
//! `frame_text` is shared across the topic submodules ([`device_render`],
//! [`process_render`]); the projection group stays inline because it
//! carries its own self-contained helpers.

use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::ui::render;
use crate::{TuiApp, TuiTheme};

fn frame_text(app: &TuiApp, width: u16, height: u16) -> String {
    // Pin English and serialize against the language-flipping i18n test.
    // render() resolves chrome through the process-global t(), which
    // otherwise auto-seeds from the host locale (a zh host would translate
    // these assertions) and a concurrent set_language(Zh) would leak
    // translated text mid-render.
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

#[path = "tests/cpu_metrics_render.rs"]
mod cpu_metrics_render;
#[path = "tests/detail_scroll_render.rs"]
mod detail_scroll_render;
#[path = "tests/device_history_render.rs"]
mod device_history_render;
#[path = "tests/device_render.rs"]
mod device_render;
#[path = "tests/directory_usage_render.rs"]
mod directory_usage_render;
#[path = "tests/process_render.rs"]
mod process_render;
#[path = "tests/session_render.rs"]
mod session_render;
#[path = "tests/source_render.rs"]
mod source_render;
#[path = "tests/startup_render.rs"]
mod startup_render;
#[path = "tests/unit_prefs_render.rs"]
mod unit_prefs_render;
#[path = "tests/visibility.rs"]
mod visibility;

// ── Projection render: partial telemetry must keep last complete values ─────
// The shell state machine folds projections into the render model; this test
// renders the TUI frame to prove a partial projection never zeroes the gauge.
mod projection_render_tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use taskmanager_application::{
        CpuMetrics, CpuTelemetryObservation, PlatformEventBatch, SystemTelemetryDomainEvent,
        SystemTelemetryProjection, SystemTelemetryProjectionApplyResult, SystemTelemetryRevision,
    };

    use crate::TuiTheme;
    use crate::render;

    fn projection_from(
        revision: SystemTelemetryRevision,
        events: impl IntoIterator<Item = SystemTelemetryDomainEvent>,
    ) -> taskmanager_application::ProjectedSystemTelemetry {
        let mut reducer = SystemTelemetryProjection::default();
        reducer.begin(revision);
        let mut latest = None;
        for event in events {
            latest = match reducer.apply(&event) {
                SystemTelemetryProjectionApplyResult::AppliedPartial(projection)
                | SystemTelemetryProjectionApplyResult::AppliedTerminal { projection } => {
                    Some(*projection)
                }
                SystemTelemetryProjectionApplyResult::Ignored(reason) => {
                    panic!("fixture projection rejected: {reason:?}")
                }
            };
        }
        latest.expect("fixture should contain an event")
    }

    fn partial_projection(
        revision: u64,
        observed_at_ms: u64,
    ) -> taskmanager_application::ProjectedSystemTelemetry {
        let revision = SystemTelemetryRevision::new(revision);
        projection_from(
            revision,
            [SystemTelemetryDomainEvent::Cpu {
                revision,
                observation: Box::new(CpuTelemetryObservation::current(
                    CpuMetrics::default(),
                    observed_at_ms,
                    Vec::new(),
                )),
            }],
        )
    }

    #[test]
    fn partial_projection_render_keeps_last_complete_values_not_zeroes() {
        let mut app = crate::TuiApp::from_shell(taskmanager_shell::demo_app());
        let mut batch = PlatformEventBatch::default();
        batch
            .system_telemetry_projections
            .push(partial_projection(1, 20));
        app.apply_platform_batch(batch);
        let backend = TestBackend::new(120, 36);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| render(frame, &app, TuiTheme::default()))
            .expect("draw");
        let text = terminal.backend().to_string();

        assert!(text.contains("37.4%"));
        assert!(!text.contains("CPU 0.0%"));
    }
}

#[test]
fn collecting_frame_masks_page_data_until_the_shared_commit_is_ready() {
    use taskmanager_application::AppPage;

    let mut app = crate::demo_app();
    let committed = app
        .projection()
        .snapshot
        .clone()
        .expect("demo fixture starts with a committed frame");
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(None)),
    );
    app.application.active_page = AppPage::Applications;

    assert!(app.telemetry_frame_state().is_collecting());
    let collecting = frame_text(&app, 120, 36);
    assert!(collecting.contains("Initializing system telemetry"));
    assert!(
        !collecting.contains("zed"),
        "partial process facts must stay behind the first-frame mask"
    );

    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(committed))),
    );
    assert!(app.telemetry_frame_state().is_ready());
    let ready = frame_text(&app, 120, 36);
    assert!(
        ready.contains("zed"),
        "the committed frame must reveal page data"
    );
    assert!(!ready.contains("Initializing system telemetry"));
}

mod table_window_tests {
    use super::super::table_window;
    use ratatui::layout::Rect;

    #[test]
    fn table_window_tracks_the_global_cursor_without_exceeding_the_viewport() {
        let area = Rect::new(0, 0, 120, 20);
        // Four rows are table chrome (borders, header, header margin), so a
        // 20-line table materializes at most sixteen body rows.
        assert_eq!(
            table_window(10_000, 0, area),
            super::super::TableWindow {
                start: 0,
                end: 16,
                selected: 0,
            }
        );
        assert_eq!(
            table_window(10_000, 5_000, area),
            super::super::TableWindow {
                start: 4_992,
                end: 5_008,
                selected: 8,
            }
        );
        assert_eq!(
            table_window(10_000, usize::MAX, area),
            super::super::TableWindow {
                start: 9_984,
                end: 10_000,
                selected: 15,
            }
        );
    }

    #[test]
    fn table_window_clamps_small_tables_and_empty_inputs() {
        let area = Rect::new(0, 0, 80, 10);
        assert_eq!(
            table_window(2, 99, area),
            super::super::TableWindow {
                start: 0,
                end: 2,
                selected: 1,
            }
        );
        assert_eq!(
            table_window(0, 0, area),
            super::super::TableWindow {
                start: 0,
                end: 0,
                selected: 0,
            }
        );
    }
}
