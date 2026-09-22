//! Parity with the neutral process-details VM: the overlay's row fold
//! and performance captions must carry the VM's values (row counts,
//! first-row identity, per-field values) — Iced only adds labels, its
//! CPU width-6 alignment, and its drop-on-missing row policy.
use super::*;
use std::path::PathBuf;
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
    observations.start_token =
        taskmanager_core::core::metrics::ScalarObservation::available(600, 42);
    observations.memory_pss_bytes =
        taskmanager_core::core::metrics::ScalarObservation::available(50 * 1024 * 1024, 42);
    observations.swap_bytes =
        taskmanager_core::core::metrics::ScalarObservation::available(2 * 1024 * 1024, 42);
    observations.disk_read_bytes_total =
        taskmanager_core::core::metrics::ScalarObservation::available(10 * 1024 * 1024, 42);
    observations.disk_write_bytes_total =
        taskmanager_core::core::metrics::ScalarObservation::available(20 * 1024 * 1024, 42);
    item.apply_scalar_observations(observations);
    item
}

fn local_time_rules() -> taskmanager_core::core::time::LocalTimeRulesObservation {
    taskmanager_core::core::time::LocalTimeRulesObservation::current(
        taskmanager_core::core::time::LocalTimeRules::utc(),
        0,
    )
}

fn vm_value(field: ProcessDetailsField) -> String {
    let rows = details_vm(
        &fixture(),
        &taskmanager_core::core::time::LocalTimeRulesObservation::current(
            taskmanager_core::core::time::LocalTimeRules::utc(),
            0,
        ),
    );
    vm_text(&rows, field)
}

#[test]
fn observed_fault_and_huge_page_counters_reach_the_rendered_rows() {
    use taskmanager_core::core::metrics::ScalarObservation;

    // The parity fixture leaves both counter families unobserved, so the rows
    // pinned the shared dash. Observe them explicitly: the renderer's row must
    // carry the real counters, not a placeholder.
    let mut item = fixture();
    item.minor_page_faults = Some(1_234);
    item.major_page_faults = Some(7);
    let mut observations = *item.scalar_observations();
    // RSS is 100 MiB in `fixture()`; 64 MiB is therefore 64.0% RSS.
    observations.memory_anon_huge_pages_bytes = ScalarObservation::available(64 * 1024 * 1024, 42);
    item.apply_scalar_observations(observations);

    let vm = details_vm(&item, &local_time_rules());
    assert_eq!(
        vm_text(&vm, ProcessDetailsField::PageFaults),
        "1234 (I/O: 7)"
    );
    assert_eq!(
        vm_text(&vm, ProcessDetailsField::AnonHugePages),
        "64.0 MiB (64.0% RSS)"
    );

    // The renderer's row fold carries the same observed counters (the output
    // is what `property_pairs` hands the properties/overview panels).
    let pairs = property_pairs(&item, &local_time_rules());
    let value = |field: ProcessDetailsField| {
        pairs
            .iter()
            .find(|(f, _, _)| *f == field)
            .map(|(_, _, value)| value.as_str())
    };
    assert_eq!(
        value(ProcessDetailsField::PageFaults),
        Some("1234 (I/O: 7)")
    );
    assert_eq!(
        value(ProcessDetailsField::AnonHugePages),
        Some("64.0 MiB (64.0% RSS)")
    );
}

#[test]
fn property_pairs_mirror_the_neutral_vm() {
    let pairs = property_pairs(&fixture(), &local_time_rules());
    assert_eq!(pairs.len(), 24);
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
        ProcessDetailsField::Pss,
        ProcessDetailsField::Uss,
        ProcessDetailsField::AnonHugePages,
        ProcessDetailsField::Threads,
        ProcessDetailsField::NetworkRate,
        ProcessDetailsField::CancelledWriteBytes,
        ProcessDetailsField::Fds,
        ProcessDetailsField::Nice,
        ProcessDetailsField::SchedPolicy,
        ProcessDetailsField::OomScore,
        ProcessDetailsField::PageFaults,
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
    assert_eq!(overview.len(), 22);
    assert_eq!(overview.first(), Some(&ProcessDetailsField::Name));
}
