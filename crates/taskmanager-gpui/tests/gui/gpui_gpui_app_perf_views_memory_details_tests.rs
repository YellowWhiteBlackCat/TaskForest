use crate::core::FailureKind;
use crate::core::metrics::{MemoryMetrics, OptionalObservation};
use crate::gpui_app::formatting::DisplayUnits;
use taskmanager_test_support::MemoryMetricsFixtureBuilder;

use super::{compressed_swap_readout, virtual_memory_commit_readout};

#[test]
fn optional_memory_rows_require_complete_observations() {
    let unknown = MemoryMetrics::default();
    assert_eq!(
        virtual_memory_commit_readout(&unknown, DisplayUnits::default()),
        None
    );
    assert_eq!(
        compressed_swap_readout(&unknown, DisplayUnits::default()),
        None
    );

    let partial = MemoryMetricsFixtureBuilder::new()
        .current_total_bytes(8 * 1024 * 1024 * 1024)
        .committed_bytes(4 * 1024 * 1024 * 1024)
        .compressed_swap_used_bytes(512 * 1024 * 1024)
        .build();
    assert_eq!(
        virtual_memory_commit_readout(&partial, DisplayUnits::default()),
        None
    );
    assert_eq!(
        compressed_swap_readout(&partial, DisplayUnits::default()),
        None
    );
}

#[test]
fn measured_zero_is_not_treated_as_missing_memory_data() {
    let memory = MemoryMetricsFixtureBuilder::new()
        .current_total_bytes(8 * 1024 * 1024 * 1024)
        .committed_bytes(0)
        .commit_limit_bytes(1024 * 1024 * 1024)
        .compressed_swap_used_bytes(0)
        .compressed_swap_capacity_bytes(1024 * 1024 * 1024)
        .build();

    assert_eq!(
        virtual_memory_commit_readout(&memory, DisplayUnits::default()).as_deref(),
        Some("0 KiB / 1.00 GiB")
    );
    assert_eq!(
        compressed_swap_readout(&memory, DisplayUnits::default()).as_deref(),
        Some("0 KiB / 1.00 GiB")
    );
}

#[test]
fn failed_typed_truth_never_renders_optional_values() {
    let memory = MemoryMetricsFixtureBuilder::new()
        .current_total_bytes(8 * 1024 * 1024 * 1024)
        .committed_bytes_observation(OptionalObservation::unavailable(FailureKind::TimedOut))
        .commit_limit_bytes_observation(OptionalObservation::unavailable(FailureKind::TimedOut))
        .compressed_swap_used_bytes_observation(OptionalObservation::unavailable(
            FailureKind::PermissionDenied,
        ))
        .compressed_swap_capacity_bytes_observation(OptionalObservation::unavailable(
            FailureKind::PermissionDenied,
        ))
        .build();
    assert_eq!(
        virtual_memory_commit_readout(&memory, DisplayUnits::default()),
        None
    );
    assert_eq!(
        compressed_swap_readout(&memory, DisplayUnits::default()),
        None
    );
}

#[test]
fn zram_readout_appends_only_a_derivable_compression_ratio() {
    // Both mm_stat sizes current → the used/capacity pair gains the ratio.
    let measured = MemoryMetricsFixtureBuilder::new()
        .current_total_bytes(8 * 1024 * 1024 * 1024)
        .compressed_swap_used_bytes(512 * 1024 * 1024)
        .compressed_swap_capacity_bytes(1024 * 1024 * 1024)
        .compressed_swap_original_bytes(3 * 1024 * 1024 * 1024)
        .compressed_swap_compressed_bytes(1024 * 1024 * 1024)
        .build();
    assert_eq!(
        compressed_swap_readout(&measured, DisplayUnits::default()).as_deref(),
        Some(
            format!(
                "512 MiB / 1.00 GiB · {} 3.0:1",
                crate::i18n::t("mem.compression_ratio")
            )
            .as_str()
        )
    );

    // A zero compressed size cannot derive a ratio; the pair stands alone.
    let undecompressible = MemoryMetricsFixtureBuilder::new()
        .current_total_bytes(8 * 1024 * 1024 * 1024)
        .compressed_swap_used_bytes(512 * 1024 * 1024)
        .compressed_swap_capacity_bytes(1024 * 1024 * 1024)
        .compressed_swap_original_bytes(3 * 1024 * 1024 * 1024)
        .compressed_swap_compressed_bytes(0)
        .build();
    assert_eq!(
        compressed_swap_readout(&undecompressible, DisplayUnits::default()).as_deref(),
        Some("512 MiB / 1.00 GiB")
    );
}
