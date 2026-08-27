//! Parity with the neutral process-details VM: the overlay's row fold
//! and performance captions must carry the VM's values (row counts,
//! first-row identity, per-field values) — Iced only adds labels, its
//! CPU width-6 alignment, and its drop-on-missing row policy.
use super::*;
use std::path::PathBuf;
use taskmanager_application::ProcessItem;

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
    observations.start_token = taskmanager_application::ScalarObservation::available(600, 42);
    observations.memory_pss_bytes =
        taskmanager_application::ScalarObservation::available(50 * 1024 * 1024, 42);
    observations.swap_bytes =
        taskmanager_application::ScalarObservation::available(2 * 1024 * 1024, 42);
    observations.disk_read_bytes_total =
        taskmanager_application::ScalarObservation::available(10 * 1024 * 1024, 42);
    observations.disk_write_bytes_total =
        taskmanager_application::ScalarObservation::available(20 * 1024 * 1024, 42);
    item.apply_scalar_observations(observations);
    item
}

fn local_time_rules() -> taskmanager_application::LocalTimeRulesObservation {
    taskmanager_application::LocalTimeRulesObservation::current(
        taskmanager_application::LocalTimeRules::utc(),
        0,
    )
}

fn vm_value(field: ProcessDetailsField) -> String {
    let rows = details_vm(
        &fixture(),
        &taskmanager_application::LocalTimeRulesObservation::current(
            taskmanager_application::LocalTimeRules::utc(),
            0,
        ),
    );
    vm_text(&rows, field)
}

#[test]
fn property_pairs_mirror_the_neutral_vm() {
    let pairs = property_pairs(&fixture(), &local_time_rules());
    // 14 rows: the 16-field surface minus the two drop-on-missing rows
    // (none missing on this fixture) plus... every field present.
    assert_eq!(pairs.len(), 16);
    let value = |field: ProcessDetailsField| {
        pairs
            .iter()
            .find(|(f, _, _)| *f == field)
            .map(|(_, _, value)| value.clone())
    };
    for field in [
        ProcessDetailsField::Name,
        ProcessDetailsField::User,
        ProcessDetailsField::Status,
        ProcessDetailsField::Memory,
        ProcessDetailsField::Threads,
        ProcessDetailsField::Fds,
        ProcessDetailsField::Nice,
        ProcessDetailsField::ParentPid,
        ProcessDetailsField::StartTime,
        ProcessDetailsField::CpuTime,
        ProcessDetailsField::DiskReadTotal,
        ProcessDetailsField::DiskWriteTotal,
        ProcessDetailsField::Cmdline,
        ProcessDetailsField::Exe,
    ] {
        assert_eq!(
            value(field),
            Some(vm_value(field)),
            "{field:?} value must come from the VM"
        );
    }
    // The CPU row keeps Iced's width-6 alignment on the VM fold.
    assert_eq!(
        value(ProcessDetailsField::Cpu),
        Some(format!("{:>6}", "12.5%"))
    );
    assert_eq!(
        value(ProcessDetailsField::StartTime),
        Some("2020-09-13 12:26:40".to_owned())
    );
}

#[test]
fn missing_observations_follow_the_drop_and_dash_policy() {
    let pairs = property_pairs(&ProcessItem::default(), &local_time_rules());
    let fields: Vec<ProcessDetailsField> = pairs.iter().map(|(f, _, _)| *f).collect();
    // Drop-on-missing rows vanish on an empty item.
    assert!(!fields.contains(&ProcessDetailsField::Cpu));
    assert!(!fields.contains(&ProcessDetailsField::ParentPid));
    assert!(!fields.contains(&ProcessDetailsField::Exe));
    // Every other missing observation renders the shared dash.
    let dash = |field: ProcessDetailsField| {
        pairs
            .iter()
            .find(|(f, _, _)| *f == field)
            .map(|(_, _, value)| value.clone())
    };
    for field in [
        ProcessDetailsField::Memory,
        ProcessDetailsField::Threads,
        ProcessDetailsField::Nice,
        ProcessDetailsField::StartTime,
        ProcessDetailsField::CpuTime,
        ProcessDetailsField::Cmdline,
    ] {
        assert_eq!(dash(field), Some(MISSING_VALUE.to_owned()), "{field:?}");
    }
}

#[test]
fn overview_exactly_the_property_rows_minus_command_and_exe() {
    let all = property_pairs(&fixture(), &local_time_rules());
    let overview: Vec<ProcessDetailsField> = all
        .iter()
        .map(|(f, _, _)| *f)
        .filter(|f| !matches!(f, ProcessDetailsField::Cmdline | ProcessDetailsField::Exe))
        .collect();
    assert_eq!(overview.len(), 14);
    assert_eq!(overview.first(), Some(&ProcessDetailsField::Name));
}
