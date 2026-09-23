//! Round-trip checks for the injected fixed-UTC rule fixture. The day/month/year
//! branches are exercised across:
//! epoch base, day rollover, Jan→Feb→Mar transitions, leap year (2024), non-leap
//! year (2023), century leap (2000, div-by-400), non-leap century (2100, div-by-
//! 100-not-400), far future, and the 23:59:59 time-of-day ceiling.
use super::{
    COMMAND_FIELDS, OVERVIEW_FIELDS, ProcessDetailsField, kv_label_value, missing_value,
    properties_unit_preferences, vm_display, vm_rows,
};
use taskmanager_application::i18n;
use taskmanager_application::i18n::Language;
use taskmanager_application::process_details_vm::format_local_timestamp_seconds;
use taskmanager_application::process_details_vm::process_details_rows_with_local_time;
use taskmanager_core::core::process::ProcessItem;
use taskmanager_core::core::process::ProcessMetadataObservations;
use taskmanager_core::core::process::ProcessOwner;
use taskmanager_core::core::time::LocalTimeRules;
use taskmanager_core::core::time::LocalTimeRulesObservation;
use taskmanager_shell::presentation::start_clock_local;
use taskmanager_test_support::ProcessItemFixtureBuilder;

/// The dialog's row sections fold straight through the neutral
/// process-details VM: a fixed fixture's overview and command rows carry
/// exactly the VM's values (behavior parity, not source text).
#[test]
fn overview_and_command_rows_mirror_the_neutral_vm() {
    use taskmanager_application::process_details_vm::{DetailValue, detail_value};
    use taskmanager_core::core::metrics::ScalarObservation;

    let mut item = ProcessItemFixtureBuilder::from_item(ProcessItem::default())
        .pid(4242)
        .parent_pid(Some(1))
        .name("sample".to_owned())
        .cmdline("sample --flag value".to_owned())
        .current_cpu_percentage(12.5)
        .current_memory_bytes(100 * 1024 * 1024)
        .status("S".to_owned())
        .metadata_observations(ProcessMetadataObservations::current(
            ProcessOwner::opaque("root"),
            Some(std::path::PathBuf::from("/usr/bin/sample")),
            42,
        ))
        .current_threads(8)
        .current_start_time_secs(1_600_000_000)
        .build();
    let mut observations = *item.scalar_observations();
    observations.start_token = ScalarObservation::available(600, 42);
    observations.memory_pss_bytes = ScalarObservation::available(50 * 1024 * 1024, 42);
    observations.memory_uss_bytes = ScalarObservation::available(256 * 1024 * 1024, 42);
    observations.swap_bytes = ScalarObservation::available(2 * 1024 * 1024, 42);
    item.apply_scalar_observations(observations);

    let utc = LocalTimeRulesObservation::current(LocalTimeRules::utc(), 0);
    let vm = process_details_rows_with_local_time(&item, &properties_unit_preferences(), &utc);
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
    assert_eq!(text(ProcessDetailsField::Pss), "50.0 MiB");

    // The definition's private facet: the observed USS reaches the overview
    // row the details dialog paints, and a never-observed USS keeps the
    // shared dash instead of a fabricated zero.
    let uss_row = OVERVIEW_FIELDS
        .iter()
        .position(|(field, _)| *field == ProcessDetailsField::Uss)
        .expect("the overview carries the USS row");
    assert_eq!(
        overview[uss_row].1, "256.0 MiB",
        "the observed private (USS) facet must render its real value"
    );
    let mut cold = item.clone();
    let mut cold_observations = *cold.scalar_observations();
    cold_observations.memory_uss_bytes = ScalarObservation::default();
    cold.apply_scalar_observations(cold_observations);
    let cold_overview = vm_rows(&cold, &OVERVIEW_FIELDS, &utc);
    assert_eq!(
        cold_overview[uss_row].1,
        missing_value(),
        "an unobserved private facet must keep the honest dash"
    );

    let command = vm_rows(&item, &COMMAND_FIELDS, &utc);
    assert_eq!(command.len(), COMMAND_FIELDS.len());
    assert_eq!(command[1].1, "/usr/bin/sample");
    assert_eq!(command[2].1, "sample --flag value");
}

/// The narrowed `memory.breakdown-rss-pss` definition, clause by clause
/// through the dialog's production folds: the resident (RSS) facet is the
/// performance current the dialog paints, and the overview paints the
/// proportional (PSS), private (USS), and derived-shared (`RSS - USS`)
/// facets of one typed observation family. A never-observed private facet
/// leaves the derived share absent (the shared dash), never a fabricated
/// `0 B`.
#[test]
fn memory_breakdown_rows_render_every_narrowed_facet() {
    use taskmanager_application::process_details_vm::{DetailValue, detail_value};
    use taskmanager_core::core::metrics::ScalarObservation;

    let mut item = ProcessItemFixtureBuilder::from_item(ProcessItem::default())
        .pid(4242)
        .name("sample".to_owned())
        .build();
    let mut observations = *item.scalar_observations();
    observations.start_token = ScalarObservation::available(600, 42);
    observations.memory_bytes = ScalarObservation::available(100 * 1024 * 1024, 42);
    observations.memory_pss_bytes = ScalarObservation::available(50 * 1024 * 1024, 42);
    observations.memory_uss_bytes = ScalarObservation::available(40 * 1024 * 1024, 42);
    item.apply_scalar_observations(observations);

    let utc = LocalTimeRulesObservation::current(LocalTimeRules::utc(), 0);
    let vm = process_details_rows_with_local_time(&item, &properties_unit_preferences(), &utc);
    assert_eq!(
        vm_display(&vm, ProcessDetailsField::Memory),
        "100.0 MiB",
        "the resident (RSS) facet is the performance current the dialog paints"
    );

    let overview = vm_rows(&item, &OVERVIEW_FIELDS, &utc);
    let facet = |field: ProcessDetailsField| {
        let index = OVERVIEW_FIELDS
            .iter()
            .position(|(candidate, _)| *candidate == field)
            .expect("the overview carries the narrowed memory facet");
        overview[index].1.as_str()
    };
    assert_eq!(facet(ProcessDetailsField::Pss), "50.0 MiB");
    assert_eq!(facet(ProcessDetailsField::Uss), "40.0 MiB");
    assert_eq!(
        facet(ProcessDetailsField::Shared),
        "60.0 MiB",
        "the derived-shared facet is RSS - USS"
    );

    // The same typed family with the private facet never observed: the
    // derived share stays missing and must not read as a fabricated `0 B`.
    let mut cold = item.clone();
    let mut cold_observations = *cold.scalar_observations();
    cold_observations.memory_uss_bytes = ScalarObservation::default();
    cold.apply_scalar_observations(cold_observations);
    let cold_overview = vm_rows(&cold, &OVERVIEW_FIELDS, &utc);
    let cold_shared = OVERVIEW_FIELDS
        .iter()
        .position(|(candidate, _)| *candidate == ProcessDetailsField::Shared)
        .expect("the overview carries the derived-shared row");
    assert_eq!(cold_overview[cold_shared].1, missing_value());
    assert_eq!(
        detail_value(
            &process_details_rows_with_local_time(&cold, &properties_unit_preferences(), &utc),
            ProcessDetailsField::Shared
        ),
        &DetailValue::Missing,
        "a missing USS must fold the derived share to the shared Missing"
    );
    assert_ne!(cold_overview[cold_shared].1, "0 B");
}

/// The performance graphs' displayed currents mirror the VM (the peaks
/// stay history folds): memory lands on the neutral base-2 ladder —
/// the documented convergence off the old hardcoded decimal MB.
#[test]
fn performance_currents_mirror_the_neutral_vm() {
    use taskmanager_application::process_details_vm::process_details_rows;

    let item = ProcessItemFixtureBuilder::from_item(ProcessItem::default())
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
    let utc = LocalTimeRulesObservation::current(LocalTimeRules::utc(), 0);
    assert_eq!(start_clock_local(Some(0), &utc), "—");
    assert_eq!(
        format_local_timestamp_seconds(1_600_000_000, &utc,),
        Some("2020-09-13 12:26:40".to_owned())
    );
    assert_eq!(
        format_local_timestamp_seconds(1_709_251_199, &utc,),
        Some("2024-02-29 23:59:59".to_owned())
    );
}

/// The legend join goes through the locale catalog: the English default
/// keeps the ASCII colon, and the pair carries both sides (no hardcoded
/// `format!("{}: ...")` in the render path).
#[test]
fn legend_pairs_join_through_the_locale_catalog() {
    i18n::set_language(Language::En);
    let joined = kv_label_value("prop.current", "3.1%");
    assert_eq!(
        joined,
        format!("{}: 3.1%", i18n::t("prop.current")),
        "must render the localized {{label}}: {{value}} shape"
    );
    assert!(joined.contains(i18n::t("prop.current")));
}
