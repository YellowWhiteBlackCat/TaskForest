//! Canonical category-tree render tests.

use super::frame_text;
use taskmanager_application::{AppAction, AppPage};
use taskmanager_core::core::process::{ProcessApplicationIdentity, ProcessMetadataObservation};

fn category_app() -> crate::TuiApp {
    let identity = ProcessApplicationIdentity::new("org.example.Editor", "Editor", None)
        .expect("identity fixture");
    let mut app = crate::demo_app();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![
            taskmanager_test_support::ProcessItemFixtureBuilder::new()
                .pid(11)
                .name("editor".into())
                .current_cpu_percentage(24.8)
                .application_identity_observation(ProcessMetadataObservation::available(
                    identity, 10,
                ))
                .build(),
            taskmanager_test_support::ProcessItemFixtureBuilder::new()
                .pid(30)
                .name("daemon".into())
                .current_cpu_percentage(1.0)
                .application_identity_observation(ProcessMetadataObservation::absent(10))
                .build(),
        ])),
    );
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    app
}

#[test]
fn category_headers_and_application_total_render_in_one_hierarchy() {
    let mut app = category_app();
    app.expanded_groups.extend([
        "category:application".to_string(),
        "category:background".to_string(),
    ]);
    let text = frame_text(&app, 150, 36);
    assert!(text.contains("Applications"));
    assert!(text.contains("Background"));
    assert!(text.contains("Editor"));
    assert!(text.contains("daemon"));
}

#[test]
fn collapsed_category_hides_process_rows_without_hiding_its_total() {
    let mut app = category_app();
    app.expanded_groups.clear();
    let text = frame_text(&app, 150, 36);
    assert!(text.contains("Applications"));
    assert!(text.contains("Background"));
    assert!(!text.contains("daemon"));
}

#[test]
fn selected_application_total_renders_the_honest_group_hint() {
    let mut app = category_app();
    app.expanded_groups = ["category:application".to_string()].into_iter().collect();
    app.selected = 1;
    app.sync_grouped_application_selection();
    let text = frame_text(&app, 150, 36);
    assert!(text.contains("Editor"));
    assert!(app.selected_detail_process().is_none());
}
