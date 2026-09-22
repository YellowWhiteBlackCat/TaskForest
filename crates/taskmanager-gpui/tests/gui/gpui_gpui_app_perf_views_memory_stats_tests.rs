//! Swap-throughput row contract for the Performance Memory page's stat rail
//! (`memory_stats::memory_page_stats`, the exact row vector `perf_views`
//! feeds into the shared `stats_panel`).
//!
//! The feature's `delivery_definition` is the system swap-in/swap-out
//! throughput pair from the shared memory projection; the swap-used/total
//! occupancy row is a distinct fact and never stands in for it.

use super::memory_page_stats;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::{
    MemoryMetrics, MemoryOptionalObservations, MemoryScalarObservations, ScalarObservation,
};
use taskmanager_core::core::units::UnitPreferences;

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;

fn memory_with(scalars: MemoryScalarObservations) -> MemoryMetrics {
    MemoryMetrics::from_observations(scalars, MemoryOptionalObservations::default())
}

fn row_value(rows: &[taskmanager_shell::viewmodel::StatRow], key: &'static str) -> Option<String> {
    rows.iter()
        .find(|row| row.label() == taskmanager_application::i18n::t(key))
        .and_then(|row| row.value().map(str::to_owned))
}

fn row_present(rows: &[taskmanager_shell::viewmodel::StatRow], key: &'static str) -> bool {
    rows.iter()
        .any(|row| row.label() == taskmanager_application::i18n::t(key))
}

/// The observable throughput pair, clause by clause: an accepted swap-in and
/// swap-out rate each render their own production row with the real formatted
/// rate, and the occupancy row keeps the used/total pair as a separate fact.
#[test]
fn swap_throughput_rows_render_the_observed_system_rates() {
    taskmanager_test_support::pin_english();
    let memory = memory_with(MemoryScalarObservations {
        total_bytes: ScalarObservation::available(16 * GIB, 10),
        used_bytes: ScalarObservation::available(6 * GIB, 10),
        swap_total_bytes: ScalarObservation::available(2 * GIB, 10),
        swap_used_bytes: ScalarObservation::available(512 * MIB, 10),
        swap_in_bytes_per_sec: ScalarObservation::available(2 * MIB, 10),
        swap_out_bytes_per_sec: ScalarObservation::available(512 * 1024, 10),
        ..Default::default()
    });
    let stats = memory_page_stats(&memory, UnitPreferences::default());
    assert_eq!(
        row_value(&stats.rows, "mem.swap_in_rate").as_deref(),
        Some("2.0 MiB/s"),
        "the observed swap-in rate must reach its production row verbatim"
    );
    assert_eq!(
        row_value(&stats.rows, "mem.swap_out_rate").as_deref(),
        Some("512.0 KiB/s"),
        "the observed swap-out rate must reach its production row verbatim"
    );
    // The occupancy fact is separate and must not be mistaken for throughput.
    assert_eq!(
        row_value(&stats.rows, "mem.swap").as_deref(),
        Some("512.0 MiB / 2.0 GiB")
    );
    assert!(stats.has_swap);
}

/// Honest absence: an unobserved (or explicitly unavailable) rate leaves no
/// row at all — the panel can never paint a fabricated `0 B/s` throughput for
/// a host whose swap-in/out counters were never sampled.
#[test]
fn unobserved_swap_throughput_leaves_no_fabricated_zero_rate() {
    taskmanager_test_support::pin_english();
    let cold = memory_with(MemoryScalarObservations {
        total_bytes: ScalarObservation::available(16 * GIB, 10),
        swap_total_bytes: ScalarObservation::available(2 * GIB, 10),
        swap_used_bytes: ScalarObservation::available(512 * MIB, 10),
        ..Default::default()
    });
    let stats = memory_page_stats(&cold, UnitPreferences::default());
    for key in ["mem.swap_in_rate", "mem.swap_out_rate"] {
        assert!(
            !row_present(&stats.rows, key),
            "{key}: an unobserved rate must not fabricate a row"
        );
    }

    let unavailable = memory_with(MemoryScalarObservations {
        swap_in_bytes_per_sec: ScalarObservation::unavailable(FailureKind::TimedOut),
        swap_out_bytes_per_sec: ScalarObservation::unavailable(FailureKind::TimedOut),
        ..Default::default()
    });
    let stats = memory_page_stats(&unavailable, UnitPreferences::default());
    for key in ["mem.swap_in_rate", "mem.swap_out_rate"] {
        assert!(
            !row_present(&stats.rows, key),
            "{key}: a typed unavailable observation must not become a zero rate"
        );
    }
}
