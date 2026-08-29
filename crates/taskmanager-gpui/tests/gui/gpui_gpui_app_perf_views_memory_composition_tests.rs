use taskmanager_core::core::FailureKind;
use taskmanager_core::core::metrics::{
    MemoryCompositionObservations, MemoryMetrics, MemoryOptionalObservations,
    MemoryScalarObservations, OptionalObservation, ScalarObservation,
};
use taskmanager_theme::Theme;

use super::memory_segments;

fn memory(composition: MemoryCompositionObservations, used: u64) -> MemoryMetrics {
    MemoryMetrics::from_observations(
        MemoryScalarObservations {
            total_bytes: ScalarObservation::available(100, 1),
            used_bytes: ScalarObservation::available(used, 1),
            available_bytes: ScalarObservation::available(100_u64.saturating_sub(used), 1),
            ..Default::default()
        },
        MemoryOptionalObservations {
            composition,
            ..Default::default()
        },
    )
}

#[test]
fn optional_composition_never_turns_partial_data_into_zero_segments() {
    let memory = memory(
        MemoryCompositionObservations {
            cached_bytes: OptionalObservation::present(10, 1),
            buffers_bytes: OptionalObservation::present(5, 1),
            active_bytes: OptionalObservation::present(20, 1),
            ..Default::default()
        },
        40,
    );
    assert_eq!(memory_segments(&memory, &Theme::dark()).len(), 3);
}

#[test]
fn complete_measured_zero_composition_is_distinct_from_unknown() {
    let measured = memory(
        MemoryCompositionObservations {
            buffers_bytes: OptionalObservation::present(0, 1),
            active_bytes: OptionalObservation::present(0, 1),
            inactive_bytes: OptionalObservation::present(0, 1),
            free_bytes: OptionalObservation::present(0, 1),
            reclaimable_bytes: OptionalObservation::present(0, 1),
            ..Default::default()
        },
        0,
    );
    let unknown = memory(Default::default(), 0);
    assert_eq!(memory_segments(&measured, &Theme::dark()).len(), 5);
    assert_eq!(memory_segments(&unknown, &Theme::dark()).len(), 2);
}

#[test]
fn failed_typed_composition_does_not_render_segments() {
    let memory = memory(
        MemoryCompositionObservations::unavailable(FailureKind::PermissionDenied),
        40,
    );
    assert_eq!(memory_segments(&memory, &Theme::dark()).len(), 2);
}
