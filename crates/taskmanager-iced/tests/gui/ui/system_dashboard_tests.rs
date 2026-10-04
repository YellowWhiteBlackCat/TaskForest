// test-intent: behavior
//! Headless behavior tests for the System-page dashboard segment: the
//! history-window label mapping, the honest summary fold (absence is a dash,
//! never zero), and render coverage across every window selection.

use super::*;
use taskmanager_application::system_timeline::SystemHistoryWindow;
use taskmanager_shell::SystemProjectionStore;

#[test]
fn window_labels_resolve_to_localized_distinct_copy() {
    let labels: Vec<&'static str> = SystemHistoryWindow::ALL
        .iter()
        .map(|window| history_window_label(*window))
        .collect();
    assert_eq!(labels.len(), 4);
    for label in &labels {
        assert!(!label.is_empty());
    }
    assert_eq!(labels, vec!["1m", "5m", "15m", "60m"]);
}

#[test]
fn summary_fold_never_fabricates_zero_from_absence() {
    let projection = SystemProjectionStore::default();
    let model = summary_model(&projection, &TimelineSeries::default());
    assert_eq!(model.cpu, "—", "an unobserved CPU renders the dash");
    assert_eq!(model.memory, "—", "an unobserved memory renders the dash");
    assert_eq!(model.processes, None, "no inventory means no count");
    assert_eq!(
        model.active_alerts, 0,
        "an empty live mirror is a real zero"
    );
}

#[test]
fn summary_fold_tracks_the_live_projection() {
    let app = crate::IcedApp::demo();
    let model = summary_model(
        app.shell.projection(),
        &app.shell
            .system_timeline_series(app.system_dashboard_window),
    );
    assert_eq!(
        model.processes,
        app.shell
            .projection()
            .processes
            .as_ref()
            .map(|processes| processes.len()),
        "the processes card mirrors the projection's inventory count"
    );
    assert_eq!(
        model.active_alerts,
        app.shell.projection().alert_active.len(),
        "the alerts card mirrors the shell's live evaluation mirror"
    );
    // Whatever the demo snapshot observes, the CPU fold is either the honest
    // dash or a formatted percentage — never a fabricated zero from absence.
    if app.shell.projection().snapshot.is_none() {
        assert_eq!(model.cpu, "—");
    } else {
        assert!(model.cpu == "—" || model.cpu.ends_with('%'));
    }
}

#[test]
fn segment_renders_for_every_window_selection_without_panic() {
    let app = crate::IcedApp::demo();
    for window in SystemHistoryWindow::ALL {
        let _ = render_system_dashboard(&app, window);
    }
}

#[test]
fn normal_dashboard_controls_select_the_same_hour_ring_and_reuse_native_graph_allocations() {
    use crate::app::{Message, SystemDashboardMessage};
    use std::rc::Rc;
    use taskmanager_application::{
        AppAction, AppPage,
        system_timeline::{SystemPageSection, TimelineMetric},
    };
    use taskmanager_shell::fixture::dashboard_history::seed_shell_system_dashboard_history;
    let mut app = crate::IcedApp::demo();
    let _ = app
        .shell
        .apply_action(AppAction::SelectPage(AppPage::System));
    assert!(seed_shell_system_dashboard_history(
        &mut app.shell,
        7_200_000
    ));
    let _ = app.update(Message::SystemDashboard(
        SystemDashboardMessage::SelectSection(SystemPageSection::Dashboard),
    ));
    assert_eq!(app.system_section, SystemPageSection::Dashboard);
    for window in SystemHistoryWindow::ALL {
        let _ = app.update(Message::SystemDashboard(
            SystemDashboardMessage::SelectWindow(window),
        ));
        assert_eq!(app.system_dashboard_window, window);
        let series = app.shell.system_timeline_series(window);
        for metric in TimelineMetric::ALL {
            assert_eq!(series.coverage_ms(metric), window.minutes() * 60_000);
            let first = app.system_timeline_graph(&series, metric);
            let second = app.system_timeline_graph(&series, metric);
            assert!(Rc::ptr_eq(&first, &second));
            assert!(first.iter().filter(|value| value.is_finite()).count() > 1);
        }
    }
    let _ = app.update(Message::SystemDashboard(
        SystemDashboardMessage::SelectSection(SystemPageSection::Hardware),
    ));
    assert_eq!(app.system_section, SystemPageSection::Hardware);
}

#[test]
fn dashboard_metric_groups_fit_allocated_viewports_with_measured_fonts() {
    use iced::advanced::{
        layout::{Layout, Limits},
        renderer::Headless,
        widget::{Id, Operation, Tree},
    };
    use iced::{Font, Pixels, Rectangle, Size};
    use taskmanager_application::system_timeline::TimelineMetric;
    use taskmanager_shell::fixture::dashboard_history::seed_shell_system_dashboard_history;
    #[derive(Default)]
    struct Groups(Vec<(TimelineMetric, Rectangle)>);
    impl Operation for Groups {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
        fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
            for metric in TimelineMetric::ALL {
                if id == Some(&Id::from(format!("system-dashboard-card-{}", metric.id()))) {
                    self.0.push((metric, bounds));
                }
            }
        }
    }
    let renderer = iced::futures::executor::block_on(iced::Renderer::new(
        Font::DEFAULT,
        Pixels(16.0),
        Some("tiny-skia"),
    ))
    .expect("software renderer");
    for (width, height) in [
        (480.0, 360.0),
        (720.0, 480.0),
        (1280.0, 720.0),
        (1600.0, 360.0),
        (480.0, 960.0),
        (800.0, 254.0),
    ] {
        let mut app = crate::IcedApp::demo();
        assert!(seed_shell_system_dashboard_history(
            &mut app.shell,
            7_200_000
        ));
        let mut visited = Vec::new();
        for first in 0..4 {
            app.system_dashboard_first_metric = first;
            let mut view = render_system_dashboard(&app, SystemHistoryWindow::SixtyMinutes);
            let mut tree = Tree::new(&view);
            let node = view.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &Limits::new(Size::ZERO, Size::new(width, height)),
            );
            let mut groups = Groups::default();
            view.as_widget_mut()
                .operate(&mut tree, Layout::new(&node), &renderer, &mut groups);
            assert!(
                !groups.0.is_empty(),
                "a complete metric group fits {width}x{height}"
            );
            for (metric, rect) in groups.0 {
                visited.push(metric);
                assert!(rect.width > 0.0 && rect.height > 0.0);
                assert!(rect.x >= 0.0 && rect.y >= 0.0);
                assert!(
                    rect.x + rect.width <= width + 0.5 && rect.y + rect.height <= height - 4.0,
                    "whole {metric:?} group {rect:?} exceeds {width}x{height}"
                );
            }
        }
        for metric in TimelineMetric::ALL {
            assert!(visited.contains(&metric), "paging reaches {metric:?}");
        }
    }
}
