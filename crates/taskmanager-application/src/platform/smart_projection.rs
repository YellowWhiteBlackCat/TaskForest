//! Revision-gated projection of the newest complete SMART job batch.
//!
//! `SmartObservationProjection` applies `SmartObservationBatch` values with a
//! monotonic-revision anti-resurrection rule shared across all frontends.

use std::collections::HashSet;

use taskmanager_core::core::identity::{DeviceGeneration, DeviceId};
use taskmanager_core::core::system_health::SmartSelfTestObservation;

use super::{SmartObservationBatch, SmartStateRevision};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmartProjectionApplyResult {
    Applied,
    IgnoredStaleOrDuplicateRevision,
    RejectedDuplicateTarget,
}

/// Application-owned projection of the newest complete SMART job batch.
///
/// Native control and observation lanes may publish out of order. Keeping the
/// revision gate here gives every frontend the same anti-resurrection rule
/// instead of asking each toolkit to interpret concurrent provider timing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SmartObservationProjection {
    revision: SmartStateRevision,
    observations: Vec<SmartSelfTestObservation>,
}

impl SmartObservationProjection {
    #[must_use]
    pub const fn revision(&self) -> SmartStateRevision {
        self.revision
    }

    #[must_use]
    pub fn observations(&self) -> &[SmartSelfTestObservation] {
        &self.observations
    }

    pub fn apply(&mut self, batch: &SmartObservationBatch) -> SmartProjectionApplyResult {
        if batch.revision <= self.revision {
            return SmartProjectionApplyResult::IgnoredStaleOrDuplicateRevision;
        }
        if has_duplicate_target(&batch.observations) {
            return SmartProjectionApplyResult::RejectedDuplicateTarget;
        }

        let mut observations = batch.observations.clone();
        observations.sort_by(|left, right| {
            (&left.device_id, left.device_generation, &left.device_key).cmp(&(
                &right.device_id,
                right.device_generation,
                &right.device_key,
            ))
        });
        self.revision = batch.revision;
        self.observations = observations;
        SmartProjectionApplyResult::Applied
    }
}

fn has_duplicate_target(observations: &[SmartSelfTestObservation]) -> bool {
    let mut targets = HashSet::<(DeviceId, DeviceGeneration)>::new();
    observations.iter().any(|observation| {
        !targets.insert((observation.device_id.clone(), observation.device_generation))
    })
}

#[cfg(test)]
#[path = "../../tests/headless/application_platform_smart_projection_tests.rs"]
mod tests;
