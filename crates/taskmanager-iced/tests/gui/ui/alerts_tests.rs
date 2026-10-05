use super::*;
use crate::app::Message;
use taskmanager_application::AlertRuleImportMode;
use taskmanager_application::ManagedAlertRule;
use taskmanager_application::ManagedAlertRuleEdit;
use taskmanager_application::i18n::{Language, set_language};
use taskmanager_assets::embedded_fonts;
use taskmanager_core::core::alerts::{AlertMetric, AlertRule};
use taskmanager_shell::fixture::ProjectionSeedFact;
use taskmanager_shell::fixture::seed_projection_fact;

fn pin_english() {
    set_language(Language::En);
}

fn opened_demo() -> crate::IcedApp {
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::Alerts(AlertsMessage::OpenPage));
    app
}

#[test]
fn rule_rows_mirror_the_shared_rules_with_typed_current_values() {
    pin_english();
    let app = opened_demo();
    let rows = rule_rows(&app);

    assert_eq!(
        rows.len(),
        app.shell.projection().alert_center.managed_rules().len(),
        "one row per shell rule"
    );
    assert_eq!(rows[0].metric_label, "CPU usage");
    assert_eq!(rows[0].threshold_text, "90.0%");
    // The demo snapshot's honest CPU observation (37.4%), not a zero.
    assert_eq!(rows[0].current_text, "37.4%");
    // Memory is derived from the fixture's used/total bytes (~39.5%).
    assert_eq!(rows[1].metric_label, "Memory usage");
    assert_eq!(rows[1].current_text, "39.5%");
    // Disk-family rules show their scope, not an ambiguous disk value.
    assert_eq!(rows[2].metric_label, "Disk temperature");
    assert_eq!(rows[2].current_text, "All disks");
    assert!(rows.iter().all(|row| row.enabled));
}

#[test]
fn unobserved_metrics_render_none_never_zero() {
    pin_english();
    let mut app = crate::IcedApp::default();
    // No snapshot at all: the typed accessors are absent, so the rows
    // must carry the localized None marker instead of a fabricated 0.
    let _ = app.update(Message::Alerts(AlertsMessage::OpenPage));
    let rows = rule_rows(&app);
    assert_eq!(rows[0].current_text, "None");
    assert!(
        !rows.iter().any(|row| row.current_text == "0.0%"),
        "no fabricated zero may render for an unobserved metric"
    );
}

#[test]
fn toggling_flips_the_row_switch_state() {
    pin_english();
    let mut app = opened_demo();
    assert!(rule_rows(&app)[0].enabled);
    let _ = app.update(Message::Alerts(AlertsMessage::ToggleRule {
        rule_id: "cpu-high".into(),
    }));
    assert!(!rule_rows(&app)[0].enabled);
}

#[test]
fn active_alert_lines_name_the_metric_value_and_severity() {
    pin_english();
    let mut app = crate::IcedApp::demo();
    // One zero-duration CPU rule at 30% (below the fixture's 37.4%) so a
    // single evaluation fires.
    app.shell
        .edit_alert_rules(ManagedAlertRuleEdit::Import {
            rules: vec![ManagedAlertRule::new(
                AlertRule::new(
                    "cpu-hot",
                    AlertMetric::CpuUsagePercent,
                    AlertSeverity::Warning,
                    30.0,
                    std::time::Duration::ZERO,
                    5.0,
                ),
                true,
            )],
            mode: AlertRuleImportMode::Replace,
        })
        .unwrap();
    let snapshot = app
        .shell
        .projection()
        .snapshot
        .clone()
        .expect("demo snapshot fixture");
    let evaluation = app.shell.evaluate_alerts(&snapshot, snapshot.timestamp_ms);
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::ActiveAlerts(evaluation.active),
    );
    assert_eq!(app.shell.projection().alert_active.len(), 1);

    let _ = app.update(Message::Alerts(AlertsMessage::OpenPage));
    let lines = active_alert_lines(&app);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].severity, AlertSeverity::Warning);
    assert!(
        lines[0].text.contains("CPU usage"),
        "the banner must name the rule metric: {}",
        lines[0].text
    );
    assert!(lines[0].text.contains("37.4"));
    assert!(lines[0].text.contains("Warning"));
}

#[test]
fn an_empty_rule_set_renders_the_localized_empty_state() {
    pin_english();
    let mut app = crate::IcedApp::demo();
    app.shell
        .edit_alert_rules(ManagedAlertRuleEdit::Import {
            rules: Vec::new(),
            mode: AlertRuleImportMode::Replace,
        })
        .unwrap();
    let _ = app.update(Message::Alerts(AlertsMessage::OpenPage));

    assert!(rule_rows(&app).is_empty());
    assert_eq!(empty_state_text(), "No alert rules configured.");
}

#[test]
fn the_alerts_route_renders_in_the_root_view() {
    pin_english();
    let mut app = crate::IcedApp::demo();
    // Both route branches must construct the real element tree.
    let _ = crate::ui::view(&app);
    let _ = app.update(Message::Alerts(AlertsMessage::OpenPage));
    assert!(app.alerts_page_open());
    let _ = crate::ui::view(&app);
}

#[test]
fn rule_cards_wrap_inside_the_viewport_while_the_toolbar_and_scroll_body_stay_bounded() {
    use iced::advanced::layout::{Layout, Limits};
    use iced::advanced::renderer::Headless;
    use iced::advanced::widget::operation::{Focusable, Scrollable};
    use iced::advanced::widget::{Id, Operation, Tree};
    use iced::{Pixels, Rectangle, Size, Vector};
    #[derive(Default)]
    struct Bounds {
        controls: Vec<(Option<Id>, Rectangle)>,
        bodies: Vec<Rectangle>,
    }
    impl Operation for Bounds {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
        fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, _: &mut dyn Focusable) {
            self.controls.push((id.cloned(), bounds));
        }
        fn scrollable(
            &mut self,
            _: Option<&Id>,
            bounds: Rectangle,
            _: Rectangle,
            _: Vector,
            _: &mut dyn Scrollable,
        ) {
            self.bodies.push(bounds);
        }
    }
    {
        let mut fonts = iced::advanced::graphics::text::font_system()
            .write()
            .expect("fonts");
        for font in embedded_fonts() {
            fonts.load_font(font);
        }
    }
    let renderer = iced::futures::executor::block_on(iced::Renderer::new(
        crate::theme_binding::BUNDLED_UI_FONT,
        Pixels(16.0),
        Some("tiny-skia"),
    ))
    .expect("renderer");
    for (width, height) in [
        (480.0, 360.0),
        (720.0, 480.0),
        (1280.0, 720.0),
        (1600.0, 360.0),
        (480.0, 960.0),
        (1920.0, 1080.0),
    ] {
        let app = opened_demo();
        let mut view = render(&app);
        let mut tree = Tree::new(&view);
        let size = Size::new(width, height);
        let node =
            view.as_widget_mut()
                .layout(&mut tree, &renderer, &Limits::new(Size::ZERO, size));
        let mut bounds = Bounds::default();
        view.as_widget_mut()
            .operate(&mut tree, Layout::new(&node), &renderer, &mut bounds);
        assert_eq!(bounds.bodies.len(), 1);
        let body = bounds.bodies[0];
        assert!(body.height > 0.0 && body.y + body.height <= height + 0.5);
        assert!(bounds.controls.len() >= 4 + 5 * 11);
        for (id, rect) in bounds.controls {
            assert!(rect.width > 0.0 && rect.height > 0.0);
            assert!(
                rect.x >= -0.5 && rect.x + rect.width <= width + 0.5,
                "{id:?}: {rect:?} at {size:?}"
            );
            if [
                FocusTarget::AlertsAdd,
                FocusTarget::AlertsExport,
                FocusTarget::AlertsImport,
                FocusTarget::AlertsImportReplace,
            ]
            .into_iter()
            .any(|target| id == Some(Id::from(focus::focus_id(target))))
            {
                assert!(rect.y >= -0.5 && rect.y + rect.height <= body.y + 0.5);
            }
        }
    }
}
