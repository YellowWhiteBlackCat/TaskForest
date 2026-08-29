//! Parity with the neutral process-details VM for the inline detail
//! panel: row count and per-row values (including the two-field joins
//! and the verified-start wrapper) must equal the VM fold.
use super::*;
use std::path::PathBuf;
use taskmanager_application::process_details_vm::{
    DetailValue, detail_value, process_details_rows,
};
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process::ProcessItem;

fn fixture() -> ProcessItem {
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
            taskmanager_core::core::process::ProcessMetadataObservations::current(
                taskmanager_core::core::process::ProcessOwner::opaque("root"),
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
    item.apply_scalar_observations(observations);
    item
}

fn vm(field: taskmanager_application::process_details_vm::ProcessDetailsField) -> String {
    let rows = process_details_rows(&fixture(), &UnitPreferences::default());
    match detail_value(&rows, field) {
        DetailValue::Text(text) => text.clone(),
        DetailValue::Missing => "—".to_owned(),
    }
}

#[test]
fn panel_rows_join_neutral_vm_values() {
    let pairs = detail_panel_pairs_with_local_time(
        &fixture(),
        None,
        &taskmanager_core::core::time::LocalTimeRulesObservation::unsupported(0),
    );
    assert_eq!(pairs.len(), 14);
    use taskmanager_application::process_details_vm::ProcessDetailsField;
    assert_eq!(pairs[0].1, vm(ProcessDetailsField::Name));
    assert_eq!(pairs[1].1, vm(ProcessDetailsField::Pid));
    assert_eq!(pairs[2].1, vm(ProcessDetailsField::User));
    assert_eq!(pairs[3].1, vm(ProcessDetailsField::Status));
    assert_eq!(
        pairs[4].1,
        format!(
            "{} / {}",
            vm(ProcessDetailsField::Cpu),
            vm(ProcessDetailsField::Memory)
        ),
        "cpu/memory row joins the two VM values"
    );
    assert_eq!(
        pairs[5].1,
        format!(
            "{} / {}",
            vm(ProcessDetailsField::Pss),
            vm(ProcessDetailsField::Swap)
        ),
        "pss/swap row joins the two VM values"
    );
    assert_eq!(
        pairs[6].1,
        format!(
            "{} / {}",
            vm(ProcessDetailsField::Threads),
            vm(ProcessDetailsField::Fds)
        ),
        "threads/fd row joins the two VM values"
    );
    assert_eq!(pairs[7].1, vm(ProcessDetailsField::CpuTime));
    assert_eq!(pairs[8].1, vm(ProcessDetailsField::Nice));
    assert_eq!(pairs[9].1, vm(ProcessDetailsField::DiskReadRate));
    assert_eq!(pairs[10].1, vm(ProcessDetailsField::DiskWriteRate));
    assert_eq!(pairs[11].1, vm(ProcessDetailsField::StartTime));
    assert_eq!(pairs[12].1, vm(ProcessDetailsField::Exe));
    assert_eq!(pairs[13].1, vm(ProcessDetailsField::Cmdline));
}

#[test]
fn verified_start_wraps_the_vm_timestamp() {
    let pairs = detail_panel_pairs_with_local_time(
        &fixture(),
        None,
        &taskmanager_core::core::time::LocalTimeRulesObservation::current(
            taskmanager_core::core::time::LocalTimeRules::utc(),
            0,
        ),
    );
    assert_eq!(pairs[11].1, "2020-09-13 12:26:40");
    // Without a frozen identity there is no verification note.
    assert!(!pairs[11].1.contains("token verified"));
}

#[test]
fn missing_observations_render_dashes_never_fabricated_values() {
    let pairs = detail_panel_pairs_with_local_time(
        &ProcessItem::default(),
        None,
        &taskmanager_core::core::time::LocalTimeRulesObservation::unsupported(0),
    );
    assert_eq!(pairs.len(), 14);
    for (index, expected) in [(4, "— / —"), (5, "— / —"), (6, "— / —")] {
        assert_eq!(pairs[index].1, expected, "row {index} joins two dashes");
    }
    for index in [7, 8, 9, 10, 11, 12, 13] {
        assert_eq!(pairs[index].1, "—", "row {index} must render the dash");
    }
    // Identity fields stay honest Text even on an empty row.
    assert_eq!(pairs[0].1, "");
    assert_eq!(pairs[1].1, "0");
}
