use crate::ui::applications::aggregate_projection::project_group_aggregate_cells;
use crate::ui::process_projection::{ProcessProjection, ProjectedRow};
use std::collections::HashSet;
use taskmanager_application::i18n::t;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::{ScalarAvailability, ScalarObservation};
use taskmanager_core::core::process::{
    ProcessApplicationIdentity, ProcessItem, ProcessMetadataObservation,
};
use taskmanager_core::core::time::LocalTimeRulesObservation;
use taskmanager_shell::ProcessRowAggregate;
use taskmanager_shell::presentation::control_error_detail;
use taskmanager_shell::{SortCol, SortDir};
use taskmanager_test_support::{ProcessItemFixtureBuilder, pin_english};

fn app_process(pid: u32, cpu: ScalarObservation<f32>) -> ProcessItem {
    let identity = ProcessApplicationIdentity::new(
        format!("org.example.aggregate{pid}"),
        format!("aggregate-{pid}"),
        None,
    )
    .expect("fixture application identity");
    let mut process = ProcessItemFixtureBuilder::new()
        .pid(pid)
        .name(format!("aggregate-{pid}"))
        .build();
    let mut observations = *process.scalar_observations();
    observations.cpu_percentage = cpu;
    process.apply_scalar_observations(observations);
    process.apply_application_identity(ProcessMetadataObservation::available(identity, 10));
    process
}

fn project_applications(
    cpu_observations: impl IntoIterator<Item = ScalarObservation<f32>>,
) -> ProcessProjection {
    let items = cpu_observations
        .into_iter()
        .enumerate()
        .map(|(index, cpu)| app_process(index as u32 + 1, cpu))
        .collect::<Vec<_>>();
    let flat = items.iter().collect::<Vec<_>>();
    let expanded_groups = HashSet::new();
    let expanded_tree = HashSet::new();
    let local_time = LocalTimeRulesObservation::unsupported(0);
    ProcessProjection::project_with_local_time(
        &flat,
        (SortCol::Cpu, SortDir::Desc),
        &expanded_groups,
        &expanded_tree,
        &local_time,
        10,
    )
}

fn applications_metrics(projection: &ProcessProjection) -> &ProcessRowAggregate {
    projection
        .rows()
        .iter()
        .find_map(|row| match row {
            ProjectedRow::GroupHeader { name, metrics, .. } if name == "Applications" => {
                Some(metrics.as_ref())
            }
            ProjectedRow::GroupHeader { .. } | ProjectedRow::Tree { .. } => None,
        })
        .expect("Applications group header")
}

#[test]
fn applications_aggregate_row_preserves_measured_zero() {
    pin_english();
    let projection = project_applications([
        ScalarObservation::available(0.0, 10),
        ScalarObservation::available(0.0, 10),
    ]);
    let metrics = applications_metrics(&projection);
    let cells = project_group_aggregate_cells(metrics);

    assert_eq!(metrics.cpu().availability(), ScalarAvailability::Available);
    assert_eq!(metrics.cpu().current_value(), Some(&0.0));
    assert_eq!(cells.cpu.text.trim(), "0.0%");
    assert!(cells.cpu.is_current_zero);
    assert!(!cells.cpu.has_detail);
}

#[test]
fn applications_aggregate_row_shows_partial_value_coverage_and_permission_reason() {
    pin_english();
    let projection = project_applications([
        ScalarObservation::available(12.5, 10),
        ScalarObservation::unavailable(FailureKind::PermissionDenied),
    ]);
    let metrics = applications_metrics(&projection);
    let cells = project_group_aggregate_cells(metrics);
    let cell = &cells.cpu;

    assert_eq!(
        metrics.cpu().availability(),
        ScalarAvailability::Partial(FailureKind::PermissionDenied)
    );
    assert_eq!(metrics.cpu().current_value(), Some(&12.5));
    assert!(cell.text.contains("12.5%"));
    assert!(cell.text.contains("1/2 current"));
    assert!(cell.text.contains(t("disk.usage_partial")));
    assert_eq!(cell.compact_text, "12.5%P1/2");
    assert!(cell.has_detail);
    assert!(
        cell.text
            .contains(control_error_detail(FailureKind::PermissionDenied))
    );
}

#[test]
fn applications_aggregate_row_marks_last_known_values_stale() {
    pin_english();
    let projection = project_applications([
        ScalarObservation::stale(4.5, 5, FailureKind::TimedOut),
        ScalarObservation::unavailable(FailureKind::TimedOut),
    ]);
    let metrics = applications_metrics(&projection);
    let cells = project_group_aggregate_cells(metrics);
    let cell = &cells.cpu;

    assert_eq!(
        metrics.cpu().availability(),
        ScalarAvailability::Stale(FailureKind::TimedOut)
    );
    assert_eq!(metrics.cpu().current_value(), None);
    assert_eq!(metrics.cpu().last_known_value(), Some(&4.5));
    assert!(cell.text.contains("4.5%"));
    assert!(cell.text.contains("1/2 known"));
    assert!(cell.text.contains(t("device.stale")));
    assert_eq!(cell.compact_text, "4.5%S1/2");
    assert!(
        cell.text
            .contains(control_error_detail(FailureKind::TimedOut))
    );
    assert!(!cell.is_current_zero);
    assert!(cell.has_detail);
}

#[test]
fn applications_aggregate_row_shows_unknown_without_a_number() {
    pin_english();
    let projection =
        project_applications([ScalarObservation::default(), ScalarObservation::default()]);
    let metrics = applications_metrics(&projection);
    let cells = project_group_aggregate_cells(metrics);

    assert_eq!(metrics.cpu().availability(), ScalarAvailability::Unknown);
    assert_eq!(cells.cpu.text, t("proc_insights.unknown"));
    assert_eq!(cells.cpu.compact_text, t("proc_insights.unknown"));
    assert!(!cells.cpu.is_current_zero);
    assert!(!cells.cpu.has_detail);
}

#[test]
fn applications_aggregate_row_shows_unavailable_reason_without_a_number() {
    pin_english();
    let projection = project_applications([
        ScalarObservation::unavailable(FailureKind::TimedOut),
        ScalarObservation::unavailable(FailureKind::TimedOut),
    ]);
    let metrics = applications_metrics(&projection);
    let cells = project_group_aggregate_cells(metrics);

    assert_eq!(
        metrics.cpu().availability(),
        ScalarAvailability::Unavailable(FailureKind::TimedOut)
    );
    assert_eq!(
        cells.cpu.text,
        format!(
            "{}: {}",
            t("proc_insights.unavailable"),
            control_error_detail(FailureKind::TimedOut)
        )
    );
    assert_eq!(cells.cpu.compact_text, t("proc_insights.unavailable"));
    assert!(!cells.cpu.is_current_zero);
    assert!(cells.cpu.has_detail);
}
