use super::*;
use taskmanager_core::core::diagnostics::{
    DiagnosticBundleErrorKind, DiagnosticBundlePlan, DiagnosticSource,
};
use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};

#[test]
fn a_normal_diagnostic_action_opens_a_real_sanitized_review() {
    let mut app = crate::demo_app();
    app.open_diagnostic_bundle();
    let Some(TuiSurface::DiagnosticBundle(view)) = app.local_surface() else {
        panic!("review must open")
    };
    let DiagnosticBundleUiState::Preview(plan) = &view.state else {
        panic!("observed fixture must prepare")
    };
    assert_eq!(plan.preview().files.len(), 4);
    assert!(plan.sanitized_contents("services.json").is_some());
    assert!(plan.sanitized_contents("startup.json").is_some());
    assert!(
        app.diagnostics.active_mut().is_none(),
        "opening a review never composes a writer"
    );
    app.confirm_diagnostic_bundle();
    assert!(
        matches!(app.local_surface(), Some(TuiSurface::DiagnosticBundle(view))
        if matches!(&view.state, DiagnosticBundleUiState::Failed(error) if error.kind() == DiagnosticBundleErrorKind::Unavailable))
    );
    app.close_local_overlays();
    assert!(app.local_surface().is_none());
}

#[test]
fn missing_observations_open_a_typed_failure_instead_of_a_healthy_report() {
    let mut app = crate::demo_app();
    seed_projection_fact(&mut app.shell, ProjectionSeedFact::Snapshot(Box::new(None)));
    app.open_diagnostic_bundle();
    assert!(
        matches!(app.local_surface(), Some(TuiSurface::DiagnosticBundle(view))
        if matches!(&view.state, DiagnosticBundleUiState::Failed(error) if error.kind() == DiagnosticBundleErrorKind::Unavailable))
    );
}

#[test]
fn compact_review_keeps_complete_actions_and_supports_end_scrolling() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use taskmanager_application::i18n::{Language, set_language};
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("language guard");
    set_language(Language::En);
    let mut app = crate::demo_app();
    app.open_diagnostic_bundle();
    let Some(TuiSurface::DiagnosticBundle(view)) = app.local_surface_mut() else {
        panic!("review")
    };
    let contents = format!(
        "{}\nFINAL-DIAGNOSTIC-END",
        "dense observed fact\n".repeat(20)
    );
    view.state = DiagnosticBundleUiState::Preview(
        DiagnosticBundlePlan::prepare(
            vec![DiagnosticSource {
                name: "facts.txt".into(),
                contents,
            }],
            [],
        )
        .expect("plan"),
    );
    view.scroll = usize::MAX;
    let mut terminal = Terminal::new(TestBackend::new(54, 16)).expect("terminal");
    terminal
        .draw(|frame| render_diagnostic_bundle_at(frame, view, TuiTheme::default(), frame.area()))
        .expect("draw");
    let text = terminal.backend().to_string();
    assert!(text.contains("Enter Export bundle"));
    assert!(text.contains("Esc Cancel"));
    assert!(
        text.contains("FINAL-DIAGNOSTIC-END"),
        "the final frozen source must be reachable by scrolling: {text}"
    );
}
