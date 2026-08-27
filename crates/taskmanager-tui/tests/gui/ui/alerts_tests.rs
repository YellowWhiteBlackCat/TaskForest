use super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use taskmanager_application::{
    AppAction, AppPage, CpuMetrics, CpuScalarObservations, ScalarObservation, SystemSnapshot,
};

use crate::demo_app;

fn frame_text(app: &crate::TuiApp, width: u16, height: u16) -> String {
    // Pin English and serialize against the language-flipping i18n test
    // (see ui::LANG_TEST_GUARD). The overlay title resolves through the
    // process-global t(), which otherwise auto-seeds from the host locale.
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| {
            render_suggestions_overlay(frame, app, crate::TuiTheme::default(), frame.area())
        })
        .expect("draw");
    terminal.backend().to_string()
}

#[test]
fn overlay_renders_title_basis_header_and_close_hint() {
    let mut app = demo_app();
    app.toggle_suggestions();
    let text = frame_text(&app, 120, 36);
    assert!(text.contains("Threshold suggestions"));
    // Basis header names the principled floor and the required floor.
    assert!(text.contains("Principled floor"));
    assert!(text.contains("samples required"));
    // Every metric label is listed.
    assert!(text.contains("CPU usage"));
    assert!(text.contains("Memory usage"));
    assert!(text.contains("Disk temperature"));
    assert!(text.contains("SMART critical warning"));
    // Close hint is shown.
    assert!(text.contains("T / Esc"));
}

#[test]
fn insufficient_metric_shows_honest_marker_not_a_fabricated_threshold() {
    // The demo app seeds exactly one telemetry tick, so CPU usage (1/20) is
    // honestly below the principled floor. Render the overlay over a page
    // that does NOT itself print CPU% so the only place 37.4 could appear
    // is a fabricated threshold.
    let mut app = demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::System));
    app.toggle_suggestions();
    let text = frame_text(&app, 120, 36);

    // Locate the CPU row specifically so the assertion is scoped to its
    // verdict rather than to any other metric's honest insufficient marker.
    let cpu_line = text
        .lines()
        .find(|line| line.contains("CPU usage"))
        .expect("the CPU usage row must render in the overlay");

    // The typed insufficient marker is present with its honest ratio.
    assert!(
        cpu_line.contains("insufficient · too_few_samples"),
        "an insufficient metric must render its typed marker"
    );
    assert!(
        cpu_line.contains("1/20"),
        "the demo's single CPU sample must show as (1/20)"
    );
    // No fabricated threshold leaks: the demo CPU value (37.4) is NOT shown
    // as a threshold number behind the typed marker.
    assert!(
        !cpu_line.contains("37.4"),
        "an insufficient metric must not render a fabricated threshold value"
    );

    // The binary SMART-warning metric names itself unsupported, not a number.
    let smart_line = text
        .lines()
        .find(|line| line.contains("SMART critical warning"))
        .expect("the SMART warning row must render in the overlay");
    assert!(
        smart_line.contains("unsupported_metric"),
        "the binary SMART metric must render its unsupported marker"
    );
    assert!(
        !smart_line.contains('%'),
        "an unsupported metric must not render a fabricated percentage threshold"
    );
}

#[test]
fn suggested_metric_shows_its_threshold_value_and_derivation() {
    // Accumulate a principled window: 25 flat CPU samples at 50% -> a
    // suggested threshold of 50.0 (mean 50, stddev 0, p95 50, low band).
    let mut app = crate::TuiApp::new();
    let snapshot = SystemSnapshot {
        cpu: CpuMetrics::from_observations(CpuScalarObservations {
            global_usage_pct: ScalarObservation::available(50.0, 1),
            ..Default::default()
        }),
        ..SystemSnapshot::default()
    };
    for _ in 0..25 {
        app.alert_suggestions.record_snapshot(&snapshot);
    }
    let _ = app.apply_action(AppAction::SelectPage(AppPage::System));
    app.toggle_suggestions();
    let text = frame_text(&app, 120, 36);

    // Scope to the CPU row: other metrics legitimately stay insufficient
    // (they have no samples), so a global contains check would be wrong.
    let cpu_line = text
        .lines()
        .find(|line| line.contains("CPU usage"))
        .expect("the CPU usage row must render in the overlay");

    // The suggested threshold value, its clear-band hysteresis, the
    // derivation basis, and the honest sample count are all surfaced.
    assert!(
        cpu_line.contains("50.0%"),
        "a suggested CPU threshold must render its value (50.0%)"
    );
    assert!(
        cpu_line.contains("±2.5 clear"),
        "the suggested clear-band hysteresis must be rendered"
    );
    assert!(
        cpu_line.contains("mean+3σ∧p95"),
        "the basis must be rendered"
    );
    assert!(
        cpu_line.contains("n=25"),
        "the honest observed sample count must be rendered"
    );
    // A metric with enough samples must NOT carry the insufficient marker.
    assert!(
        !cpu_line.contains("insufficient"),
        "a metric above the sample floor must not render the insufficient marker"
    );
}
