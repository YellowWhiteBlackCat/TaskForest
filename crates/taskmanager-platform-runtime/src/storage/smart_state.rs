//! Target-keyed SMART job state owned by the shared storage runtime.

use std::collections::HashMap;
use std::sync::Mutex;

use taskmanager_application::{
    DeviceGeneration, DeviceId, ProviderFailure, SmartSelfTestObservation, SmartStateRevision,
    StorageDeviceTarget,
};

/// Idle SMART jobs are retained long enough for ordinary refresh jitter while
/// still allowing abandoned or timed-out tracking state to expire.
pub const DEFAULT_SMART_JOB_RETENTION_MS: u64 = 30_000;
/// Maximum independently retained SMART jobs in one runtime.
///
/// Physical identity remains authoritative and an existing device may replace
/// its own generation/job at the ceiling. The bound prevents fabricated or
/// churned device IDs from growing TTL-retained state without limit.
pub const MAX_TRACKED_SMART_JOBS: usize = 1_024;

/// Stable physical lifecycle identity used to partition SMART jobs.
///
/// Native locators are deliberately excluded: they are revalidated I/O
/// addresses, not lifecycle identity.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SmartTargetKey {
    pub device_id: DeviceId,
    pub device_generation: DeviceGeneration,
}

impl SmartTargetKey {
    #[must_use]
    pub fn from_target(target: &StorageDeviceTarget) -> Self {
        Self {
            device_id: target.device_id.clone(),
            device_generation: target.device_generation,
        }
    }

    #[must_use]
    pub fn from_observation(observation: &SmartSelfTestObservation) -> Self {
        Self {
            device_id: observation.device_id.clone(),
            device_generation: observation.device_generation,
        }
    }
}

/// Opaque runtime generation for one job on one physical lifecycle target.
///
/// This token is never a device generation and never crosses the application
/// event boundary. It exists solely to reject stale concurrent commits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmartJobToken {
    pub target: SmartTargetKey,
    pub job_generation: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SmartJobSnapshot {
    pub token: SmartJobToken,
    pub observation: SmartSelfTestObservation,
    pub updated_at_ms: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SmartStateSnapshot {
    pub revision: SmartStateRevision,
    pub jobs: Vec<SmartJobSnapshot>,
}

impl SmartStateSnapshot {
    #[must_use]
    pub fn observations(&self) -> Vec<SmartSelfTestObservation> {
        self.jobs
            .iter()
            .map(|job| job.observation.clone())
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmartCommitStatus {
    Applied,
    Superseded,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SmartInstallResult {
    pub installed: SmartJobSnapshot,
    pub retired: Vec<SmartSelfTestObservation>,
}

#[derive(Debug)]
struct SmartRuntimeState {
    retention_ms: u64,
    max_jobs: usize,
    next_job_generation: u64,
    revision: SmartStateRevision,
    jobs: HashMap<SmartTargetKey, SmartJobSnapshot>,
}

impl Default for SmartRuntimeState {
    fn default() -> Self {
        Self::new(DEFAULT_SMART_JOB_RETENTION_MS)
    }
}

impl SmartRuntimeState {
    fn new(retention_ms: u64) -> Self {
        Self {
            retention_ms,
            max_jobs: MAX_TRACKED_SMART_JOBS,
            next_job_generation: 0,
            revision: SmartStateRevision::default(),
            jobs: HashMap::new(),
        }
    }

    fn snapshot(&self) -> SmartStateSnapshot {
        let mut jobs = self.jobs.values().cloned().collect::<Vec<_>>();
        jobs.sort_by(|left, right| left.token.target.cmp(&right.token.target));
        SmartStateSnapshot {
            revision: self.revision,
            jobs,
        }
    }

    fn next_revision(&self) -> Result<SmartStateRevision, ProviderFailure> {
        self.revision
            .checked_next()
            .ok_or(ProviderFailure::ProviderFault)
    }

    fn snapshot_target(&self, target: &StorageDeviceTarget) -> Option<SmartJobSnapshot> {
        self.jobs
            .get(&SmartTargetKey::from_target(target))
            .filter(|job| job.observation.target() == *target)
            .cloned()
    }

    fn install_started(
        &mut self,
        observation: SmartSelfTestObservation,
        now_ms: u64,
    ) -> Result<SmartInstallResult, ProviderFailure> {
        let target = SmartTargetKey::from_observation(&observation);
        self.ensure_start_allowed(&target)?;
        // Refuse before mutating the map. Reusing a saturated token would let
        // an arbitrarily old in-flight poll pass the generation check.
        let next_job_generation = self
            .next_job_generation
            .checked_add(1)
            .ok_or(ProviderFailure::ProviderFault)?;
        let next_revision = self.next_revision()?;
        let retired_keys = self
            .jobs
            .keys()
            .filter(|key| key.device_id == target.device_id)
            .cloned()
            .collect::<Vec<_>>();
        let retired = retired_keys
            .into_iter()
            .filter_map(|key| self.jobs.remove(&key))
            .map(|job| job.observation)
            .collect();

        self.next_job_generation = next_job_generation;
        let installed = SmartJobSnapshot {
            token: SmartJobToken {
                target: target.clone(),
                job_generation: self.next_job_generation,
            },
            observation,
            updated_at_ms: now_ms,
        };
        self.jobs.insert(target, installed.clone());
        self.revision = next_revision;
        Ok(SmartInstallResult { installed, retired })
    }

    fn can_install(&self, target: &SmartTargetKey) -> bool {
        self.jobs.len() < self.max_jobs
            || self
                .jobs
                .keys()
                .any(|existing| existing.device_id == target.device_id)
    }

    fn ensure_start_allowed(&self, target: &SmartTargetKey) -> Result<(), ProviderFailure> {
        if !self.can_install(target) {
            return Err(ProviderFailure::Rejected);
        }
        self.next_job_generation
            .checked_add(1)
            .ok_or(ProviderFailure::ProviderFault)?;
        self.next_revision()?;
        Ok(())
    }

    fn commit_observation(
        &mut self,
        token: &SmartJobToken,
        observation: SmartSelfTestObservation,
        now_ms: u64,
    ) -> Result<SmartCommitStatus, ProviderFailure> {
        let Some(current) = self.jobs.get(&token.target) else {
            return Ok(SmartCommitStatus::Superseded);
        };
        if current.token != *token
            || SmartTargetKey::from_observation(&observation) != token.target
            || current.observation.target() != observation.target()
        {
            return Ok(SmartCommitStatus::Superseded);
        }
        let next_revision = self.next_revision()?;
        let Some(current) = self.jobs.get_mut(&token.target) else {
            return Ok(SmartCommitStatus::Superseded);
        };
        current.observation = observation;
        current.updated_at_ms = current.updated_at_ms.max(now_ms);
        self.revision = next_revision;
        Ok(SmartCommitStatus::Applied)
    }

    fn remove_if_current(
        &mut self,
        token: &SmartJobToken,
    ) -> Result<Option<SmartSelfTestObservation>, ProviderFailure> {
        let Some(current) = self.jobs.get(&token.target) else {
            return Ok(None);
        };
        if current.token != *token {
            return Ok(None);
        }
        let next_revision = self.next_revision()?;
        let removed = self.jobs.remove(&token.target).map(|job| job.observation);
        self.revision = next_revision;
        Ok(removed)
    }

    fn stop_tracking(
        &mut self,
        target: &StorageDeviceTarget,
    ) -> Result<Option<SmartSelfTestObservation>, ProviderFailure> {
        let key = SmartTargetKey::from_target(target);
        let Some(current) = self.jobs.get(&key) else {
            return Ok(None);
        };
        if current.observation.target() != *target {
            return Ok(None);
        }
        let next_revision = self.next_revision()?;
        let removed = self.jobs.remove(&key).map(|job| job.observation);
        self.revision = next_revision;
        Ok(removed)
    }

    fn contains(&self, token: &SmartJobToken) -> bool {
        self.jobs
            .get(&token.target)
            .is_some_and(|current| current.token == *token)
    }

    fn prune_expired(
        &mut self,
        now_ms: u64,
    ) -> Result<Vec<SmartSelfTestObservation>, ProviderFailure> {
        let expired = self
            .jobs
            .iter()
            .filter(|(_, job)| now_ms.saturating_sub(job.updated_at_ms) > self.retention_ms)
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        if expired.is_empty() {
            return Ok(Vec::new());
        }
        let next_revision = self.next_revision()?;
        let removed = expired
            .into_iter()
            .filter_map(|key| self.jobs.remove(&key))
            .map(|job| job.observation)
            .collect();
        self.revision = next_revision;
        Ok(removed)
    }
}

/// Target-keyed, generation-checked SMART job state shared by native adapters.
#[derive(Debug)]
pub struct SharedSmartRuntimeState(Mutex<SmartRuntimeState>);

impl Default for SharedSmartRuntimeState {
    fn default() -> Self {
        Self(Mutex::new(SmartRuntimeState::default()))
    }
}

impl SharedSmartRuntimeState {
    #[must_use]
    pub fn new(retention_ms: u64) -> Self {
        Self(Mutex::new(SmartRuntimeState::new(retention_ms)))
    }

    pub fn snapshot(&self) -> Result<SmartStateSnapshot, ProviderFailure> {
        self.0
            .lock()
            .map(|state| state.snapshot())
            .map_err(|_| ProviderFailure::ProviderFault)
    }

    pub fn snapshot_target(
        &self,
        target: &StorageDeviceTarget,
    ) -> Result<Option<SmartJobSnapshot>, ProviderFailure> {
        self.0
            .lock()
            .map(|state| state.snapshot_target(target))
            .map_err(|_| ProviderFailure::ProviderFault)
    }

    pub fn install_started(
        &self,
        observation: SmartSelfTestObservation,
        now_ms: u64,
    ) -> Result<SmartInstallResult, ProviderFailure> {
        let mut state = self.0.lock().map_err(|_| ProviderFailure::ProviderFault)?;
        state.install_started(observation, now_ms)
    }

    pub(crate) fn ensure_start_capacity(
        &self,
        target: &StorageDeviceTarget,
    ) -> Result<(), ProviderFailure> {
        let state = self.0.lock().map_err(|_| ProviderFailure::ProviderFault)?;
        state.ensure_start_allowed(&SmartTargetKey::from_target(target))
    }

    pub fn commit_observation(
        &self,
        token: &SmartJobToken,
        observation: SmartSelfTestObservation,
        now_ms: u64,
    ) -> Result<SmartCommitStatus, ProviderFailure> {
        let mut state = self.0.lock().map_err(|_| ProviderFailure::ProviderFault)?;
        state.commit_observation(token, observation, now_ms)
    }

    pub fn remove_if_current(
        &self,
        token: &SmartJobToken,
    ) -> Result<Option<SmartSelfTestObservation>, ProviderFailure> {
        let mut state = self.0.lock().map_err(|_| ProviderFailure::ProviderFault)?;
        state.remove_if_current(token)
    }

    pub fn stop_tracking(
        &self,
        target: &StorageDeviceTarget,
    ) -> Result<Option<SmartSelfTestObservation>, ProviderFailure> {
        let mut state = self.0.lock().map_err(|_| ProviderFailure::ProviderFault)?;
        state.stop_tracking(target)
    }

    pub fn contains(&self, token: &SmartJobToken) -> Result<bool, ProviderFailure> {
        self.0
            .lock()
            .map(|state| state.contains(token))
            .map_err(|_| ProviderFailure::ProviderFault)
    }

    pub fn prune_expired(
        &self,
        now_ms: u64,
    ) -> Result<Vec<SmartSelfTestObservation>, ProviderFailure> {
        let mut state = self.0.lock().map_err(|_| ProviderFailure::ProviderFault)?;
        state.prune_expired(now_ms)
    }
}

#[cfg(test)]
#[path = "../../tests/headless/storage/smart_state.rs"]
mod tests;
