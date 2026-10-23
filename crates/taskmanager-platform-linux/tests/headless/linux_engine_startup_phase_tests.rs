//! test-intent: behavior
//!
//! The systemd boot summary is the only Linux source for the per-phase boot
//! durations. The tests below pin the parser against the real report shapes:
//! the full four-phase line, a line without firmware timing, the compound
//! span systemd prints for long phases, and a report that carries no phase at
//! all. A phase that was not measured must stay absent, never zero.

use super::*;

#[test]
fn the_full_summary_line_yields_every_measured_phase() {
    let summary = "Startup finished in 3.192s (firmware) + 1.281s (loader) + 6.585s (kernel) + 30.101s (userspace) = 41.159s\n";

    let phases = parse_systemd_analyze_phases(summary);

    assert_eq!(phases.firmware_ms, Some(3_192));
    assert_eq!(phases.loader_ms, Some(1_281));
    assert_eq!(phases.kernel_ms, Some(6_585));
    assert_eq!(phases.user_space_ms, Some(30_101));
    assert!(phases.has_measured_phase());
}

#[test]
fn a_phase_the_platform_did_not_measure_stays_absent() {
    // A host without firmware timing prints no firmware term at all.
    let summary =
        "Startup finished in 1.281s (loader) + 6.585s (kernel) + 30.101s (userspace) = 37.967s\n";

    let phases = parse_systemd_analyze_phases(summary);

    assert_eq!(phases.firmware_ms, None);
    assert_eq!(phases.loader_ms, Some(1_281));
    assert_eq!(phases.kernel_ms, Some(6_585));
    assert_eq!(phases.user_space_ms, Some(30_101));
}

#[test]
fn a_compound_span_is_read_as_one_measured_phase() {
    // systemd prints a compound span for long phases.
    let summary = "Startup finished in 1min 3.500s (userspace) = 1min 3.500s\n";

    let phases = parse_systemd_analyze_phases(summary);

    assert_eq!(phases.user_space_ms, Some(63_500));
    assert_eq!(phases.firmware_ms, None);
}

#[test]
fn a_report_without_any_phase_measures_nothing() {
    let summary = "Failed to get system uptime: Operation not permitted\n";

    let phases = parse_systemd_analyze_phases(summary);

    assert!(!phases.has_measured_phase());
    assert_eq!(phases.firmware_ms, None);
    assert_eq!(phases.loader_ms, None);
    assert_eq!(phases.kernel_ms, None);
    assert_eq!(phases.user_space_ms, None);
}

#[test]
fn the_trailing_total_is_never_used_as_a_phase_duration() {
    // The total also covers the initrd span, which this contract does not
    // model; borrowing it as a phase duration would claim something the
    // summary never said.
    let summary = "Startup finished in 3.192s (firmware) + 1.281s (loader) + 6.585s (kernel) + 30.101s (userspace) = 41.159s\n";

    let phases = parse_systemd_analyze_phases(summary);

    assert_eq!(phases.user_space_ms, Some(30_101));
    assert_ne!(phases.user_space_ms, Some(41_159));
    assert_eq!(phases.measured_total_ms(), 3_192 + 1_281 + 6_585 + 30_101);
}

#[test]
fn an_initrd_term_is_not_reported_as_a_measured_phase() {
    let summary = "Startup finished in 3.192s (firmware) + 1.281s (loader) + 6.585s (kernel) + 900ms (initrd) + 30.101s (userspace) = 42.059s\n";

    let phases = parse_systemd_analyze_phases(summary);

    assert_eq!(phases.firmware_ms, Some(3_192));
    assert_eq!(phases.user_space_ms, Some(30_101));
    assert_eq!(
        phases.measured_total_ms(),
        3_192 + 1_281 + 6_585 + 30_101,
        "the initrd span is not a phase this contract publishes"
    );
}

#[test]
fn a_malformed_term_leaves_its_phase_unmeasured() {
    let summary = "Startup finished in abc (firmware) + 6.585s (kernel) = 6.585s\n";

    let phases = parse_systemd_analyze_phases(summary);

    assert_eq!(phases.firmware_ms, None);
    assert_eq!(phases.kernel_ms, Some(6_585));
}

#[test]
fn a_failed_phase_source_reports_a_typed_failure_and_no_timeline() {
    let timeline = failed_timeline(
        StartupEvidenceFailure::PermissionDenied,
        DeviceStatus::PermissionDenied,
        42,
    );

    assert_eq!(timeline.state.status, DeviceStatus::PermissionDenied);
    assert_eq!(
        timeline.failure,
        Some(StartupEvidenceFailure::PermissionDenied)
    );
    assert_eq!(timeline.timeline, None);
}

#[test]
fn the_report_shape_a_live_systemd_prints_yields_the_four_published_phases() {
    // The exact report shape systemd prints on a real boot, including the
    // initrd term this contract does not model and the trailing blank.
    let report = "Startup finished in 6.075s (firmware) + 7.756s (loader) + 1.945s (kernel) + 2.374s (initrd) + 12.995s (userspace) = 31.147s \ngraphical.target reached after 12.995s in userspace.\n";

    let phases = parse_systemd_analyze_phases(report);

    assert_eq!(phases.firmware_ms, Some(6_075));
    assert_eq!(phases.loader_ms, Some(7_756));
    assert_eq!(phases.kernel_ms, Some(1_945));
    assert_eq!(phases.user_space_ms, Some(12_995));
    assert_eq!(
        phases.measured_total_ms(),
        6_075 + 7_756 + 1_945 + 12_995,
        "the initrd span is not a phase this contract publishes"
    );
}
