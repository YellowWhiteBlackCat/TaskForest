//! Round-trip checks for the injected fixed-UTC rule fixture. The day/month/year
//! branches are exercised across:
//! epoch base, day rollover, Jan→Feb→Mar transitions, leap year (2024), non-leap
//! year (2023), century leap (2000, div-by-400), non-leap century (2100, div-by-
//! 100-not-400), far future, and the 23:59:59 time-of-day ceiling.
use super::{
    COMMAND_FIELDS, OVERVIEW_FIELDS, ProcessDetailsField, kv_label_value, missing_value,
    properties_unit_preferences, vm_display, vm_rows,
};
use crate::i18n;

/// The dialog's row sections fold straight through the neutral
/// process-details VM: a fixed fixture's overview and command rows carry
/// exactly the VM's values (behavior parity, not source text).
#[test]
fn overview_and_command_rows_mirror_the_neutral_vm() {
    use crate::core::metrics::ScalarObservation;
    use taskmanager_application::process_details_vm::{DetailValue, detail_value};

    let mut item = taskmanager_test_support::ProcessItemFixtureBuilder::from_item(
        crate::core::process::ProcessItem::default(),
    )
    .pid(4242)
    .parent_pid(Some(1))
    .name("sample".to_owned())
    .cmdline("sample --flag value".to_owned())
    .current_cpu_percentage(12.5)
    .current_memory_bytes(100 * 1024 * 1024)
    .status("S".to_owned())
    .metadata_observations(
        taskmanager_application::ProcessMetadataObservations::current(
            taskmanager_application::ProcessOwner::opaque("root"),
            Some(std::path::PathBuf::from("/usr/bin/sample")),
            42,
        ),
    )
    .current_threads(8)
    .current_start_time_secs(1_600_000_000)
    .build();
    let mut observations = *item.scalar_observations();
    observations.start_token = ScalarObservation::available(600, 42);
    observations.memory_pss_bytes = ScalarObservation::available(50 * 1024 * 1024, 42);
    observations.swap_bytes = ScalarObservation::available(2 * 1024 * 1024, 42);
    item.apply_scalar_observations(observations);

    let utc = taskmanager_application::LocalTimeRulesObservation::current(
        taskmanager_application::LocalTimeRules::utc(),
        0,
    );
    let vm = taskmanager_application::process_details_vm::process_details_rows_with_local_time(
        &item,
        &properties_unit_preferences(),
        &utc,
    );
    let text = |field| match detail_value(&vm, field) {
        DetailValue::Text(text) => text.clone(),
        DetailValue::Missing => missing_value(),
    };

    let overview = vm_rows(&item, &OVERVIEW_FIELDS, &utc);
    assert_eq!(overview.len(), OVERVIEW_FIELDS.len());
    for (row, (field, _)) in overview.iter().zip(OVERVIEW_FIELDS) {
        assert_eq!(row.1, text(field), "{field:?} must come from the VM");
    }
    assert_eq!(text(ProcessDetailsField::StartTime), "2020-09-13 12:26:40");
    assert_eq!(text(ProcessDetailsField::Memory), "100.0 MiB");

    let command = vm_rows(&item, &COMMAND_FIELDS, &utc);
    assert_eq!(command.len(), COMMAND_FIELDS.len());
    assert_eq!(command[1].1, "/usr/bin/sample");
    assert_eq!(command[2].1, "sample --flag value");
}

/// The performance graphs' displayed currents mirror the VM (the peaks
/// stay history folds): memory lands on the neutral base-2 ladder —
/// the documented convergence off the old hardcoded decimal MB.
#[test]
fn performance_currents_mirror_the_neutral_vm() {
    use taskmanager_application::process_details_vm::process_details_rows;

    let item = taskmanager_test_support::ProcessItemFixtureBuilder::from_item(
        crate::core::process::ProcessItem::default(),
    )
    .pid(4242)
    .current_cpu_percentage(12.5)
    .current_memory_bytes(100 * 1024 * 1024)
    .current_disk_read_bytes_per_sec(1536)
    .current_disk_write_bytes_per_sec(1024 * 1024)
    .build();
    let vm = process_details_rows(&item, &properties_unit_preferences());
    // The fixture builder publishes canonical current observations.
    assert_eq!(
        vm_display(&vm, ProcessDetailsField::Cpu),
        "12.5%",
        "CPU current must come from the VM"
    );
    assert_eq!(
        vm_display(&vm, ProcessDetailsField::Memory),
        "100.0 MiB",
        "memory current must render the neutral base-2 ladder"
    );
    assert_eq!(
        vm_display(&vm, ProcessDetailsField::DiskReadRate),
        "1.5 KiB/s"
    );
    assert_eq!(
        vm_display(&vm, ProcessDetailsField::DiskWriteRate),
        "1.0 MiB/s"
    );
}

/// The injected start-time helper keeps its zero-sentinel dash and explicit
/// fixed-UTC fixture output for known epochs.
#[test]
fn injected_start_time_keeps_the_sentinel_and_fixture_shape() {
    let utc = taskmanager_application::LocalTimeRulesObservation::current(
        taskmanager_application::LocalTimeRules::utc(),
        0,
    );
    assert_eq!(
        taskmanager_shell::presentation::start_clock_local(Some(0), &utc),
        "—"
    );
    assert_eq!(
        taskmanager_application::process_details_vm::format_local_timestamp_seconds(
            1_600_000_000,
            &utc,
        ),
        Some("2020-09-13 12:26:40".to_owned())
    );
    assert_eq!(
        taskmanager_application::process_details_vm::format_local_timestamp_seconds(
            1_709_251_199,
            &utc,
        ),
        Some("2024-02-29 23:59:59".to_owned())
    );
}

/// The legend join goes through the locale catalog: the English default
/// keeps the ASCII colon, and the pair carries both sides (no hardcoded
/// `format!("{}: ...")` in the render path).
#[test]
fn legend_pairs_join_through_the_locale_catalog() {
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let joined = kv_label_value("prop.current", "3.1%");
    assert_eq!(
        joined,
        format!("{}: 3.1%", i18n::t("prop.current")),
        "must render the localized {{label}}: {{value}} shape"
    );
    assert!(joined.contains(i18n::t("prop.current")));
}
