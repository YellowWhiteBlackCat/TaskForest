//! Parity with the neutral process-details VM: a fixed fixture folds to
//! the same row counts and values the application-layer VM produces —
//! the TUI modal only adds labels, joins two VM fields per combined row,
//! and appends peak metadata (behavior acceptance, not source text).
use super::*;
use std::path::PathBuf;
use taskmanager_application::process_details_vm::{DetailValue, ProcessDetailsField, detail_value};
use taskmanager_application::{ProcessItem, ScalarObservation};

fn fixture() -> ProcessItem {
    // Every measurement enters through the typed fixture builder or the
    // canonical observation group; no schema-v1 row mirror participates.
    let mut item = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(4242)
        .parent_pid(Some(1))
        .name("sample".to_owned())
        .cmdline("sample --flag value".to_owned())
        .current_cpu_percentage(12.5)
        .current_memory_bytes(100 * 1024 * 1024)
        .current_disk_read_bytes_per_sec(1536)
        .current_disk_write_bytes_per_sec(1024 * 1024)
        .status("S".to_owned())
        .metadata_observations(
            taskmanager_application::ProcessMetadataObservations::current(
                taskmanager_application::ProcessOwner::opaque("root"),
                Some(PathBuf::from("/usr/bin/sample")),
                42,
            ),
        )
        .current_threads(8)
        .current_start_time_secs(1_600_000_000)
        .current_cpu_time_secs(3_690)
        .current_fds(42)
        .current_nice(10)
        .build();
    let mut observations = *item.scalar_observations();
    observations.start_token = ScalarObservation::available(600, 42);
    observations.memory_pss_bytes = ScalarObservation::available(50 * 1024 * 1024, 42);
    observations.swap_bytes = ScalarObservation::available(2 * 1024 * 1024, 42);
    observations.disk_read_bytes_total = ScalarObservation::available(10 * 1024 * 1024, 42);
    observations.disk_write_bytes_total = ScalarObservation::available(20 * 1024 * 1024, 42);
    item.apply_scalar_observations(observations);
    item
}

fn vm(field: ProcessDetailsField) -> String {
    let rows = taskmanager_application::process_details_vm::process_details_rows_with_local_time(
        &fixture(),
        &UnitPreferences::default(),
        &taskmanager_application::LocalTimeRulesObservation::current(
            taskmanager_application::LocalTimeRules::utc(),
            0,
        ),
    );
    match detail_value(&rows, field) {
        DetailValue::Text(text) => text.clone(),
        DetailValue::Missing => "—".to_owned(),
    }
}

#[test]
fn overview_rows_mirror_the_neutral_vm() {
    let pairs = overview_pairs(
        &fixture(),
        &taskmanager_application::LocalTimeRulesObservation::current(
            taskmanager_application::LocalTimeRules::utc(),
            0,
        ),
    );
    assert_eq!(pairs.len(), 7);
    let fields = [
        ProcessDetailsField::Name,
        ProcessDetailsField::Pid,
        ProcessDetailsField::ParentPid,
        ProcessDetailsField::User,
        ProcessDetailsField::Status,
        ProcessDetailsField::Threads,
        ProcessDetailsField::StartTime,
    ];
    for (row, field) in pairs.iter().zip(fields) {
        assert_eq!(row.1, vm(field), "{field:?} value must come from the VM");
    }
}

#[test]
fn command_rows_mirror_the_neutral_vm() {
    let pairs = command_pairs(
        &fixture(),
        &taskmanager_application::LocalTimeRulesObservation::current(
            taskmanager_application::LocalTimeRules::utc(),
            0,
        ),
    );
    assert_eq!(pairs.len(), 3);
    assert_eq!(pairs[0].1, vm(ProcessDetailsField::Name));
    assert_eq!(pairs[1].1, vm(ProcessDetailsField::Exe));
    assert_eq!(pairs[2].1, vm(ProcessDetailsField::Cmdline));
}

#[test]
fn performance_currents_mirror_the_neutral_vm() {
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let pairs = performance_pairs(
        &fixture(),
        &taskmanager_application::LocalTimeRulesObservation::current(
            taskmanager_application::LocalTimeRules::utc(),
            0,
        ),
    );
    assert_eq!(pairs.len(), 4);
    // With an empty history window the peak floors at the live reading,
    // so every row renders "{current} (peak {current})".
    for (row, field) in pairs.iter().zip([
        ProcessDetailsField::Cpu,
        ProcessDetailsField::Memory,
        ProcessDetailsField::DiskReadRate,
        ProcessDetailsField::DiskWriteRate,
    ]) {
        let current = vm(field);
        assert_eq!(
            row.1,
            format!("{current} (peak {current})"),
            "{field:?} current must come from the VM"
        );
    }
}
