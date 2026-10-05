//! test-intent: behavior
use super::*;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use taskmanager_core::core::alerts::{Alert, AlertEvent, AlertMetric, AlertSeverity};
#[test]
fn event_modal_filters_authoritative_history_exports_complete_json_and_clears_it() {
    let mut app = crate::demo_app();
    let alert = Alert {
        instance_id: "cpu:system".into(),
        rule_id: "cpu".into(),
        target: "CPU".into(),
        metric: AlertMetric::CpuUsagePercent,
        severity: AlertSeverity::Warning,
        value: 93.0,
        threshold: 90.0,
        active_since_ms: 1,
    };
    app.shell.replace_alert_event_history(vec![
        AlertEvent {
            id: 1,
            kind: AlertEventKind::Activated,
            alert: alert.clone(),
            observed_at_ms: 1,
        },
        AlertEvent {
            id: 2,
            kind: AlertEventKind::Cleared,
            alert,
            observed_at_ms: 2,
        },
    ]);
    app.open_local_surface(crate::TuiSurface::Health);
    app.select_health_review(HealthReviewMode::Events);
    assert_eq!(crate::ui::health_review::groups(&app).len(), 2);
    for kind in [AlertEventKind::Activated, AlertEventKind::Cleared] {
        crate::runtime::handle_key(&mut app, KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE));
        let groups = crate::ui::health_review::groups(&app);
        assert_eq!(groups.len(), 1);
        assert!(groups[0].title.contains(&format!("{kind:?}")));
    }
    let expected =
        export_alert_events_json(app.projection().alert_center.event_history()).expect("JSON");
    let mut sink = Vec::new();
    app.export_event_history_to(&mut sink);
    let mut expected_bytes = Vec::new();
    crate::clipboard::write_clipboard(&mut expected_bytes, &expected).expect("output");
    assert_eq!(
        sink, expected_bytes,
        "filter cannot omit records from JSON export"
    );
    struct Refuse;
    impl std::io::Write for Refuse {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("sink unavailable"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    app.export_event_history_to(&mut Refuse);
    assert!(
        app.shell
            .feedback_notice()
            .is_some_and(|notice| notice.text().contains("sink unavailable"))
    );
    crate::runtime::handle_key(&mut app, KeyEvent::new(KeyCode::F(8), KeyModifiers::NONE));
    assert!(app.projection().alert_center.event_history().is_empty());
    assert_eq!(app.health_review.selected, 0);
}
