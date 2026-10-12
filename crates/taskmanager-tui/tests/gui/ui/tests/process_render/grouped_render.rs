//! Canonical category-tree render tests.

use super::frame_text;
use taskmanager_application::{AppAction, AppPage};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process::{
    ProcessApplicationIdentity, ProcessItem, ProcessMetadataObservation, ProcessScalarObservations,
};
use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};
use taskmanager_test_support::{ProcessItemFixtureBuilder, pin_english};

fn category_app() -> crate::TuiApp {
    let identity = ProcessApplicationIdentity::new("org.example.Editor", "Editor", None)
        .expect("identity fixture");
    let mut app = crate::demo_app();
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::Processes(Some(vec![
            ProcessItemFixtureBuilder::new()
                .pid(11)
                .name("editor".into())
                .current_cpu_percentage(24.8)
                .application_identity_observation(ProcessMetadataObservation::available(
                    identity, 10,
                ))
                .build(),
            ProcessItemFixtureBuilder::new()
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

fn scalar_process(
    pid: u32,
    cpu: ScalarObservation<f32>,
    memory: ScalarObservation<u64>,
) -> ProcessItem {
    let mut scalars = ProcessScalarObservations::default();
    scalars.cpu_percentage = cpu;
    scalars.memory_bytes = memory;
    ProcessItemFixtureBuilder::new()
        .pid(pid)
        .name(format!("sample-{pid}"))
        .scalar_observations(scalars)
        .build()
}

fn grouped_scalar_app(processes: Vec<ProcessItem>) -> crate::TuiApp {
    let mut app = crate::demo_app();
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::Processes(Some(processes)),
    );
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    app.expanded_groups.clear();
    app.selected = 0;
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
fn selected_application_total_renders_its_aggregate_details() {
    let mut app = category_app();
    app.expanded_groups = ["category:application".to_string()].into_iter().collect();
    app.selected = 1;
    app.sync_grouped_application_selection();
    let text = frame_text(&app, 150, 36);
    assert!(text.contains("Editor"));
    assert!(text.contains("CPU"));
    assert!(app.selected_detail_process().is_none());
}

#[test]
fn group_aggregate_cells_keep_measured_zero_as_a_current_value() {
    pin_english();
    let app = grouped_scalar_app(vec![scalar_process(
        101,
        ScalarObservation::available(0.0, 10),
        ScalarObservation::available(0, 10),
    )]);

    let text = frame_text(&app, 180, 52);

    assert!(
        text.contains("0.0%"),
        "the aggregate zero is current: {text}"
    );
    assert!(
        text.contains("0 B"),
        "the aggregate memory zero is current: {text}"
    );
    assert!(
        text.lines()
            .any(|line| line.contains("CPU") && line.contains("0.0%"))
    );
    assert!(
        text.lines()
            .any(|line| line.contains("Memory") && line.contains("0 B"))
    );
}

#[test]
fn group_aggregate_cells_and_details_preserve_partial_permission_coverage() {
    pin_english();
    let app = grouped_scalar_app(vec![
        scalar_process(
            101,
            ScalarObservation::available(12.5, 10),
            ScalarObservation::available(1024, 10),
        ),
        scalar_process(
            102,
            ScalarObservation::unavailable(FailureKind::PermissionDenied),
            ScalarObservation::unavailable(FailureKind::PermissionDenied),
        ),
    ]);

    let text = frame_text(&app, 180, 52);

    assert!(
        text.contains("12%P1/2"),
        "the narrow CPU cell keeps a rounded current value and coverage: {text}"
    );
    assert!(
        text.contains("1.0KiBP1/2"),
        "partial memory keeps its current value and coverage: {text}"
    );
    assert!(
        text.contains("12.5% · 1/2 · partial: Permission denied"),
        "selected CPU details retain the exact current value, coverage, status, and reason: {text}"
    );
}

#[test]
fn group_aggregate_cells_distinguish_stale_last_known_from_current_values() {
    pin_english();
    let app = grouped_scalar_app(vec![
        scalar_process(
            101,
            ScalarObservation::stale(48.7, 8, FailureKind::PermissionDenied),
            ScalarObservation::available(1024, 10),
        ),
        scalar_process(
            102,
            ScalarObservation::unavailable(FailureKind::PermissionDenied),
            ScalarObservation::available(2048, 10),
        ),
    ]);

    let text = frame_text(&app, 180, 52);

    assert!(
        text.contains("49%S1/2"),
        "the narrow CPU cell shows the rounded last-known value as stale: {text}"
    );
    assert!(
        text.contains("48.7% · Stale data · 1/2 · Permission denied"),
        "selected CPU details retain the exact last-known value, status, coverage, and reason: {text}"
    );
    assert!(
        text.contains("3.0 KiB"),
        "the memory total remains current: {text}"
    );
    assert!(
        text.lines()
            .any(|line| line.contains("Memory") && line.contains("3.0 KiB"))
    );
}

#[test]
fn group_aggregate_cells_render_unknown_without_a_number() {
    pin_english();
    let app = grouped_scalar_app(vec![scalar_process(
        101,
        ScalarObservation::default(),
        ScalarObservation::default(),
    )]);

    let text = frame_text(&app, 180, 52);

    assert!(text.contains("Unknown"));
    let cpu_line = text
        .lines()
        .find(|line| line.contains("CPU") && line.contains("Unknown"))
        .expect("CPU aggregate is unknown");
    let memory_line = text
        .lines()
        .find(|line| line.contains("Memory") && line.contains("Unknown"))
        .expect("memory aggregate is unknown");
    assert!(!cpu_line.chars().any(|character| character.is_ascii_digit()));
    assert!(
        !memory_line
            .chars()
            .any(|character| character.is_ascii_digit())
    );
}

#[test]
fn group_aggregate_cells_render_unavailable_failure_without_a_number() {
    pin_english();
    let app = grouped_scalar_app(vec![scalar_process(
        101,
        ScalarObservation::unavailable(FailureKind::PermissionDenied),
        ScalarObservation::unavailable(FailureKind::PermissionDenied),
    )]);

    let text = frame_text(&app, 180, 52);

    assert!(
        text.contains("Unavail…"),
        "the narrow table cell marks unavailable with an ellipsis: {text}"
    );
    assert!(text.contains("Unavailable: Permission denied"));
    let cpu_line = text
        .lines()
        .find(|line| line.contains("CPU") && line.contains("Unavailable"))
        .expect("CPU aggregate failure is visible in the details panel");
    let memory_line = text
        .lines()
        .find(|line| line.contains("Memory") && line.contains("Unavailable"))
        .expect("memory aggregate failure is visible in the details panel");
    assert!(!cpu_line.chars().any(|character| character.is_ascii_digit()));
    assert!(
        !memory_line
            .chars()
            .any(|character| character.is_ascii_digit())
    );
}
