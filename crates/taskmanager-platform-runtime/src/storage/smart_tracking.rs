//! SMART tracking refresh and control over shared runtime state.
//!
//! Polls running self-test jobs, commits observations, retires jobs on
//! expiry, supersession, or identity change, and folds per-target issues into
//! a single batch health verdict.

use taskmanager_application::{
    PlatformEvent, SmartControlRequest, SmartEvent, SmartObservationBatch, SmartObservationIssue,
    SmartObservationRequest, SmartTrackingEnd, SmartTrackingEndReason,
};
use taskmanager_core::core::device_state::DeviceStatus;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::smart::{
    SmartSelfTestFailure, SmartSelfTestPhase, SmartSelfTestReport,
};
use taskmanager_core::core::storage::StorageDeviceTarget;
use taskmanager_platform_contract::ProviderFailure;

use super::smart_state::{SharedSmartRuntimeState, SmartCommitStatus, SmartJobSnapshot};
use super::{SmartControlExecutor, SmartObservationExecutor};
use crate::{CapabilityHealth, degraded_health};

pub(super) fn refresh(
    request: SmartObservationRequest,
    state: &SharedSmartRuntimeState,
    provider: &mut SmartObservationExecutor,
    now_ms: u64,
) -> Result<(PlatformEvent, CapabilityHealth), ProviderFailure> {
    let mut batch = SmartObservationBatch::default();
    batch.ended.extend(
        state
            .prune_expired(now_ms)?
            .into_iter()
            .map(|observation| ended(&observation.target(), SmartTrackingEndReason::Expired)),
    );

    let jobs = match request {
        SmartObservationRequest::RefreshAll => state.snapshot()?.jobs,
        SmartObservationRequest::RefreshTarget(target) => {
            batch.subject = Some(target.clone());
            match state.snapshot_target(&target)? {
                Some(job) => vec![job],
                None => {
                    batch.issues.push(SmartObservationIssue {
                        target,
                        failure: FailureKind::TemporarilyUnavailable,
                    });
                    Vec::new()
                }
            }
        }
    };
    let mut usable_observations = 0;
    for job in jobs {
        if refresh_job(state, provider, now_ms, job, &mut batch)? {
            usable_observations += 1;
        }
    }
    let snapshot = state.snapshot()?;
    batch.revision = snapshot.revision;
    batch.observations = snapshot.observations();
    sort_batch(&mut batch);
    let health = batch_health(&batch, usable_observations > 0);
    Ok((PlatformEvent::Smart(SmartEvent::Batch(batch)), health))
}

pub(super) fn control(
    request: SmartControlRequest,
    state: &SharedSmartRuntimeState,
    provider: &mut SmartControlExecutor,
    now_ms: u64,
) -> Result<(PlatformEvent, CapabilityHealth), ProviderFailure> {
    let mut batch = SmartObservationBatch::default();
    let mut has_usable_observation = true;
    batch.ended.extend(
        state
            .prune_expired(now_ms)?
            .into_iter()
            .map(|observation| ended(&observation.target(), SmartTrackingEndReason::Expired)),
    );
    match request {
        SmartControlRequest::StartSelfTest(intent) => {
            // Reject before invoking the drive-side mutation. A provider must
            // never start a self-test that the bounded runtime cannot track.
            state.ensure_start_capacity(&intent.target())?;
            let report = provider(&intent, now_ms)?;
            let new_generation = intent.device_generation;
            let installed = state.install_started(intent.into_observation(report), now_ms)?;
            batch.subject = Some(installed.installed.observation.target());
            if let Some(failure) = report_failure(&installed.installed.observation.report) {
                has_usable_observation = false;
                batch.issues.push(SmartObservationIssue {
                    target: installed.installed.observation.target(),
                    failure,
                });
            }
            batch
                .ended
                .extend(installed.retired.into_iter().map(|observation| {
                    let reason = if observation.device_generation == new_generation {
                        SmartTrackingEndReason::SupersededJob
                    } else {
                        SmartTrackingEndReason::DeviceGenerationChanged
                    };
                    ended(&observation.target(), reason)
                }));
        }
        SmartControlRequest::StopTracking(target) => {
            batch.subject = Some(target.clone());
            if let Some(observation) = state.stop_tracking(&target)? {
                batch.ended.push(ended(
                    &observation.target(),
                    SmartTrackingEndReason::Requested,
                ));
            }
        }
    }
    let snapshot = state.snapshot()?;
    batch.revision = snapshot.revision;
    batch.observations = snapshot.observations();
    sort_batch(&mut batch);
    let health = batch_health(&batch, has_usable_observation);
    Ok((PlatformEvent::Smart(SmartEvent::Batch(batch)), health))
}

fn refresh_job(
    state: &SharedSmartRuntimeState,
    provider: &mut SmartObservationExecutor,
    now_ms: u64,
    job: SmartJobSnapshot,
    batch: &mut SmartObservationBatch,
) -> Result<bool, ProviderFailure> {
    if job.observation.report.phase != SmartSelfTestPhase::Running {
        return state.contains(&job.token);
    }
    let target = job.observation.target();
    match provider(&target, job.observation.report.state, now_ms) {
        Ok(report) => {
            let report_failure = report_failure(&report);
            let mut observation = job.observation;
            observation.report = report;
            let applied = matches!(
                state.commit_observation(&job.token, observation, now_ms)?,
                SmartCommitStatus::Applied
            );
            if applied && let Some(failure) = report_failure {
                batch.issues.push(SmartObservationIssue { target, failure });
                return Ok(false);
            }
            return Ok(applied);
        }
        Err(ProviderFailure::IdentityChanged) => {
            if let Some(removed) = state.remove_if_current(&job.token)? {
                batch.ended.push(ended(
                    &removed.target(),
                    SmartTrackingEndReason::IdentityChanged,
                ));
                batch.issues.push(SmartObservationIssue {
                    target,
                    failure: FailureKind::IdentityChanged,
                });
            }
        }
        Err(failure) => {
            // A concurrent start/cancel may have invalidated this poll while
            // the bounded provider call was running. Never attach its failure
            // to the replacement job.
            if state.contains(&job.token)? {
                batch.issues.push(SmartObservationIssue {
                    target,
                    failure: failure.kind(),
                });
            }
        }
    }
    Ok(false)
}

fn ended(target: &StorageDeviceTarget, reason: SmartTrackingEndReason) -> SmartTrackingEnd {
    SmartTrackingEnd {
        target: target.clone(),
        reason,
    }
}

fn sort_batch(batch: &mut SmartObservationBatch) {
    batch.observations.sort_by(|left, right| {
        (&left.device_id, left.device_generation, &left.device_key).cmp(&(
            &right.device_id,
            right.device_generation,
            &right.device_key,
        ))
    });
    batch
        .issues
        .sort_by(|left, right| target_order(&left.target, &right.target));
    batch
        .ended
        .sort_by(|left, right| target_order(&left.target, &right.target));
}

fn target_order(left: &StorageDeviceTarget, right: &StorageDeviceTarget) -> std::cmp::Ordering {
    (&left.device_id, left.device_generation, &left.locator).cmp(&(
        &right.device_id,
        right.device_generation,
        &right.locator,
    ))
}

fn batch_health(batch: &SmartObservationBatch, has_usable_observation: bool) -> CapabilityHealth {
    match degraded_health(batch.issues.iter().map(|issue| issue.failure)) {
        CapabilityHealth::Degraded(failure) if !has_usable_observation => {
            CapabilityHealth::Unavailable(ProviderFailure::from_kind(failure))
        }
        health => health,
    }
}

fn report_failure(report: &SmartSelfTestReport) -> Option<FailureKind> {
    report.failure.map_or_else(
        || match report.state.status {
            DeviceStatus::Healthy => None,
            DeviceStatus::Stale => Some(FailureKind::TemporarilyUnavailable),
            DeviceStatus::PermissionDenied => Some(FailureKind::PermissionDenied),
            DeviceStatus::MissingTool => Some(FailureKind::MissingDependency),
            DeviceStatus::Unsupported => Some(FailureKind::Unsupported),
        },
        |failure| {
            Some(match failure {
                SmartSelfTestFailure::InvalidDevice => FailureKind::IdentityChanged,
                SmartSelfTestFailure::MissingTool => FailureKind::MissingDependency,
                SmartSelfTestFailure::RequiresEscalation => FailureKind::RequiresEscalation,
                SmartSelfTestFailure::PermissionDenied => FailureKind::PermissionDenied,
                SmartSelfTestFailure::TimedOut => FailureKind::TimedOut,
                SmartSelfTestFailure::ProviderUnavailable => FailureKind::TemporarilyUnavailable,
                SmartSelfTestFailure::Rejected => FailureKind::Rejected,
            })
        },
    )
}

#[cfg(test)]
#[path = "../../tests/headless/runtime_storage_smart_tracking_tests.rs"]
mod tests;
