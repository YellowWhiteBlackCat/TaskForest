//! Schema-v1 compatibility hydration for CPU observations.

use super::super::availability::hydrate_legacy_group;
use super::{ScalarAvailability, ScalarObservation, ScalarObservationGroup};
use crate::core::FailureKind;

pub(super) const fn scalar_projection<T: Copy>(observation: &ScalarObservation<T>) -> Option<T> {
    if matches!(observation.availability(), ScalarAvailability::Available) {
        observation.current_value().copied()
    } else {
        None
    }
}

pub(super) fn complete_group_projection<T: Copy>(
    group: &ScalarObservationGroup<T>,
) -> Option<Vec<T>> {
    if !matches!(group.availability(), ScalarAvailability::Available) {
        return None;
    }
    Some(
        group
            .last_known_observations()
            .iter()
            .filter_map(scalar_projection)
            .collect(),
    )
}

pub(super) fn optional_group_projection<T: Copy>(
    group: &ScalarObservationGroup<T>,
) -> Option<Vec<Option<T>>> {
    if !matches!(group.availability(), ScalarAvailability::Available) {
        return None;
    }
    Some(
        group
            .last_known_observations()
            .iter()
            .map(scalar_projection)
            .collect(),
    )
}

pub(super) fn observation_group_projection<T: Clone>(
    group: &ScalarObservationGroup<T>,
) -> Option<Vec<ScalarObservation<T>>> {
    matches!(group.availability(), ScalarAvailability::Available)
        .then(|| group.last_known_observations().to_vec())
}

pub(super) fn hydrate_scalar<T: Copy>(
    observation: ScalarObservation<T>,
    legacy: Option<T>,
) -> ScalarObservation<T> {
    if matches!(observation.availability(), ScalarAvailability::Unknown)
        && let Some(value) = legacy
    {
        return ScalarObservation::available(value, 0);
    }
    observation
}

pub(super) fn hydrate_group<T>(
    group: ScalarObservationGroup<T>,
    legacy_items: Vec<ScalarObservation<T>>,
) -> ScalarObservationGroup<T> {
    hydrate_legacy_group(group, legacy_items)
}

pub(super) fn hydrate_optional_group<T>(
    group: ScalarObservationGroup<T>,
    legacy: Vec<Option<T>>,
) -> ScalarObservationGroup<T> {
    hydrate_legacy_group(
        group,
        legacy
            .into_iter()
            .map(|value| {
                value.map_or_else(
                    || ScalarObservation::unavailable(FailureKind::Unsupported),
                    |value| ScalarObservation::available(value, 0),
                )
            })
            .collect(),
    )
}
