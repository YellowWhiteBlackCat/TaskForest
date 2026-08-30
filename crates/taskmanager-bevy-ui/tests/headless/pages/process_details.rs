//! Behavior tests for the Bevy selected-process details projection.

use taskmanager_application::process_details_vm::ProcessDetailsField;
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process::ProcessItem;

use taskmanager_shell::{ShellApp, fixture};

use super::projection;

fn shell_with(mut process: ProcessItem) -> ShellApp {
    let mut scalars = *process.scalar_observations();
    scalars.cpu_percentage = ScalarObservation::available(17.5, 1);
    scalars.memory_bytes = ScalarObservation::available(256 * 1024 * 1024, 1);
    scalars.threads = ScalarObservation::available(6, 1);
    process.apply_scalar_observations(scalars);
    let mut shell = ShellApp::new();
    fixture::edit_processes(&mut shell, |processes| *processes = Some(vec![process]));
    shell
}

#[test]
fn selected_projection_uses_the_shared_vm_and_keeps_insights_typed() {
    let shell = shell_with(ProcessItem::new(42, "worker"));
    let view = projection(&shell);

    assert_eq!(
        view.selected,
        Some(super::ProcessDetailsSelection {
            identity: None,
            pid: 42,
            name: "worker".to_owned(),
        })
    );
    let value = |field: ProcessDetailsField| {
        view.overview
            .iter()
            .find(|row| row.label == taskmanager_application::i18n::t(field_label(field)))
            .map(|row| row.value.as_str())
    };
    assert_eq!(value(ProcessDetailsField::Cpu), Some("17.5%"));
    assert_eq!(value(ProcessDetailsField::Memory), Some("256.0 MiB"));
    assert_eq!(value(ProcessDetailsField::Threads), Some("6"));
    assert_eq!(view.insights.len(), 7);
    assert!(
        view.insights
            .iter()
            .all(|card| card.value == taskmanager_application::i18n::t("proc_insights.collecting")),
        "no process-insights projection means collecting, never fabricated zeros"
    );
}

#[test]
fn empty_process_projection_is_an_explicit_unselected_state() {
    let view = projection(&ShellApp::new());
    assert_eq!(view.selected, None);
    assert!(view.overview.is_empty());
    assert!(view.insights.is_empty());
}

fn field_label(field: ProcessDetailsField) -> &'static str {
    match field {
        ProcessDetailsField::Cpu => "common.cpu",
        ProcessDetailsField::Memory => "common.memory",
        ProcessDetailsField::Threads => "common.threads",
        _ => unreachable!("test only looks up the three scalar rows above"),
    }
}
