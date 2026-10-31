//! Behavior coverage for typed group aggregate display text.

use std::collections::HashSet;

use crate::gpui_app::processes_view::rows::category_tree_rows;
use taskforest_core::core::process::{
    ProcessApplicationIdentity, ProcessItem, ProcessMetadataObservation, ProcessScalarObservations,
};
use taskforest_core::core::units::UnitPreferences;
use taskforest_core::core::{FailureKind, ScalarObservation};
use taskmanager_application::i18n::{self, Language};
use taskmanager_shell::SortCol;
use taskmanager_test_support::ProcessItemFixtureBuilder;

fn pin_english() -> Language {
    let prior = i18n::current_language();
    i18n::set_language(Language::En);
    prior
}

#[test]
fn aggregate_cells_keep_zero_coverage_freshness_and_failure_states() {
    let prior = pin_english();
    let app = ProcessApplicationIdentity::new("org.example.Editor", "Editor", None)
        .expect("fixture identity must be non-empty");
    let typed_item = |pid: u32,
                      name: &str,
                      identity: Option<ProcessApplicationIdentity>,
                      cpu: ScalarObservation<f32>,
                      memory: ScalarObservation<u64>| {
        let builder = ProcessItemFixtureBuilder::new()
            .pid(pid)
            .name(name.to_owned())
            .scalar_observations(ProcessScalarObservations {
                start_token: ScalarObservation::available(u64::from(pid) + 100, 1),
                cpu_percentage: cpu,
                memory_bytes: memory,
                ..ProcessScalarObservations::default()
            });
        match identity {
            Some(identity) => builder
                .application_identity_observation(ProcessMetadataObservation::available(
                    identity, 1,
                ))
                .build(),
            None if name == "daemon" => builder
                .application_identity_observation(ProcessMetadataObservation::<
                    ProcessApplicationIdentity,
                >::absent(1))
                .build(),
            None => builder.build(),
        }
    };
    let processes = [
        typed_item(
            101,
            "editor",
            Some(app.clone()),
            ScalarObservation::available(0.0, 1),
            ScalarObservation::available(0, 1),
        ),
        typed_item(
            102,
            "editor-helper",
            Some(app),
            ScalarObservation::unavailable(FailureKind::PermissionDenied),
            ScalarObservation::available(1024, 1),
        ),
        typed_item(
            201,
            "daemon",
            None,
            ScalarObservation::stale(7.0, 1, FailureKind::TimedOut),
            ScalarObservation::<u64>::default(),
        ),
        typed_item(
            301,
            "mystery",
            None,
            ScalarObservation::<f32>::default(),
            ScalarObservation::unavailable(FailureKind::Unsupported),
        ),
    ];
    let references: Vec<&ProcessItem> = processes.iter().collect();
    let rows = category_tree_rows(
        &references,
        42,
        SortCol::Name,
        true,
        &HashSet::new(),
        &HashSet::new(),
        UnitPreferences::default(),
    );

    let applications = &rows[0];
    assert_eq!(applications.name, "Applications");
    assert!(applications.cell_text.cpu.starts_with("0.0%"));
    assert!(applications.cell_text.cpu.contains("partial 1/2"));
    assert!(applications.cell_text.cpu.contains("Permission denied"));
    assert_eq!(applications.cell_text.cpu_compact, "0%P1/2");
    assert!(applications.cell_text.memory.starts_with("1.0 KiB"));
    assert!(!applications.cell_text.memory.contains("partial"));
    assert_eq!(applications.cell_text.memory_compact, "1KiB");

    let background = &rows[1];
    assert!(background.cell_text.cpu.starts_with("7.0%"));
    assert!(background.cell_text.cpu.contains("Stale data 1/1"));
    assert!(background.cell_text.cpu.contains("timed out"));
    assert_eq!(background.cell_text.cpu_compact, "7%S1/1");
    assert_eq!(background.cell_text.memory, "Unknown");

    let uncategorized = &rows[2];
    assert_eq!(uncategorized.cell_text.cpu, "Unknown");
    assert!(uncategorized.cell_text.memory.starts_with("Unavailable"));
    assert!(uncategorized.cell_text.memory.contains("not supported"));
    assert!(!uncategorized.cell_text.memory.contains("0 B"));
    i18n::set_language(prior);
}
