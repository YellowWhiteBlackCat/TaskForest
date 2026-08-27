//! OS-neutral filesystem-health, SMART, and directory-usage execution
//! contracts.

use std::sync::Arc;

use crossbeam_channel::Receiver;
use taskmanager_application::{
    CapabilityId, CompositeSourceSnapshot, DirectoryUsageRequest, PlatformEvent, ProviderFailure,
    SmartControlRequest, SmartObservationRequest, StorageHealthEvent, StorageHealthRequest,
};
use taskmanager_core::{
    DeviceState, DirectoryScanControl, DirectoryScanSpec, DirectoryUsageSnapshot,
    FilesystemHealthSnapshot, SmartSelfTestIntent, SmartSelfTestReport, StorageDeviceTarget,
};

use crate::{
    Queued, RuntimeEventPublisher, WorkerRuntime, WorkerSpawnError, spawn_health_observation_lane,
    spawn_observation_lane,
};

mod directory_scan;
pub use directory_scan::spawn_directory_usage_lane;
mod smart_state;
pub use smart_state::{
    DEFAULT_SMART_JOB_RETENTION_MS, MAX_TRACKED_SMART_JOBS, SharedSmartRuntimeState,
    SmartCommitStatus, SmartInstallResult, SmartJobSnapshot, SmartJobToken, SmartStateSnapshot,
    SmartTargetKey,
};
mod smart_tracking;

type FilesystemHealthExecutor = dyn FnMut(u64) -> Result<CompositeSourceSnapshot<FilesystemHealthSnapshot>, ProviderFailure>
    + Send
    + 'static;
type SmartObservationExecutor = dyn FnMut(&StorageDeviceTarget, DeviceState, u64) -> Result<SmartSelfTestReport, ProviderFailure>
    + Send
    + 'static;
type SmartControlExecutor = dyn FnMut(&SmartSelfTestIntent, u64) -> Result<SmartSelfTestReport, ProviderFailure>
    + Send
    + 'static;
type DirectoryUsageExecutor = dyn FnMut(
        &DirectoryScanSpec,
        &DirectoryScanControl,
        u64,
    ) -> Result<DirectoryUsageSnapshot, ProviderFailure>
    + Send
    + 'static;

/// Native storage operations adapted into OS-independent executor closures.
///
/// The locator remains opaque to this crate. Native adapters resolve and
/// revalidate it immediately before I/O; the shared runtime owns only request,
/// event, lifecycle, revision, and health policy. Directory usage is an
/// optional facet: `None` here matches an absent binding.
pub struct StorageExecutors {
    filesystem_health: Box<FilesystemHealthExecutor>,
    smart_observation: Box<SmartObservationExecutor>,
    smart_control: Box<SmartControlExecutor>,
    directory_usage: Option<Box<DirectoryUsageExecutor>>,
}

impl StorageExecutors {
    #[must_use]
    pub fn new<H, O, C>(filesystem_health: H, smart_observation: O, smart_control: C) -> Self
    where
        H: FnMut(u64) -> Result<CompositeSourceSnapshot<FilesystemHealthSnapshot>, ProviderFailure>
            + Send
            + 'static,
        O: FnMut(
                &StorageDeviceTarget,
                DeviceState,
                u64,
            ) -> Result<SmartSelfTestReport, ProviderFailure>
            + Send
            + 'static,
        C: FnMut(&SmartSelfTestIntent, u64) -> Result<SmartSelfTestReport, ProviderFailure>
            + Send
            + 'static,
    {
        Self {
            filesystem_health: Box::new(filesystem_health),
            smart_observation: Box::new(smart_observation),
            smart_control: Box::new(smart_control),
            directory_usage: None,
        }
    }

    /// Attach the optional directory-usage executor (mirrors the optional
    /// binding; absence means the capability is honestly unavailable).
    #[must_use]
    pub fn with_directory_usage<D>(mut self, directory_usage: D) -> Self
    where
        D: FnMut(
                &DirectoryScanSpec,
                &DirectoryScanControl,
                u64,
            ) -> Result<DirectoryUsageSnapshot, ProviderFailure>
            + Send
            + 'static,
    {
        self.directory_usage = Some(Box::new(directory_usage));
        self
    }
}

/// Optional storage receivers while native capability bindings are assembled.
pub struct PendingStorageRuntimeLanes {
    pub health_rx: Option<Receiver<Queued<StorageHealthRequest>>>,
    pub smart_observation_rx: Option<Receiver<Queued<SmartObservationRequest>>>,
    pub smart_control_rx: Option<Receiver<Queued<SmartControlRequest>>>,
    pub directory_usage_rx: Option<Receiver<Queued<DirectoryUsageRequest>>>,
}

impl PendingStorageRuntimeLanes {
    pub(crate) fn new(
        health_rx: Option<Receiver<Queued<StorageHealthRequest>>>,
        smart_observation_rx: Option<Receiver<Queued<SmartObservationRequest>>>,
        smart_control_rx: Option<Receiver<Queued<SmartControlRequest>>>,
        directory_usage_rx: Option<Receiver<Queued<DirectoryUsageRequest>>>,
    ) -> Self {
        Self {
            health_rx,
            smart_observation_rx,
            smart_control_rx,
            directory_usage_rx,
        }
    }

    pub(crate) fn health_capability_missing(&self) -> bool {
        self.health_rx.is_none()
    }

    pub(crate) fn missing_smart_capabilities(&self) -> impl Iterator<Item = CapabilityId> {
        [
            (self.smart_observation_rx.is_none(), CapabilityId::SMART),
            (self.smart_control_rx.is_none(), CapabilityId::SMART_CONTROL),
        ]
        .into_iter()
        .filter_map(|(is_missing, capability)| is_missing.then_some(capability))
    }

    /// Promote the storage family only when all three required lanes exist;
    /// the optional directory-usage lane rides along when bound.
    #[must_use]
    pub fn try_complete(self) -> Option<StorageRuntimeLanes> {
        let Self {
            health_rx: Some(health),
            smart_observation_rx: Some(smart_observation),
            smart_control_rx: Some(smart_control),
            directory_usage_rx,
        } = self
        else {
            return None;
        };
        Some(StorageRuntimeLanes {
            health,
            smart_observation,
            smart_control,
            directory_usage: directory_usage_rx,
        })
    }
}

/// Complete provider-side receivers for the storage capability family. The
/// directory-usage receiver is optional (absent = capability unavailable).
pub struct StorageRuntimeLanes {
    health: Receiver<Queued<StorageHealthRequest>>,
    smart_observation: Receiver<Queued<SmartObservationRequest>>,
    smart_control: Receiver<Queued<SmartControlRequest>>,
    directory_usage: Option<Receiver<Queued<DirectoryUsageRequest>>>,
}

/// Attach the storage executors to independent bounded lanes. The optional
/// directory-usage executor gets its own lane, so a running scan can never
/// block filesystem-health or SMART work (and vice versa).
pub fn spawn_storage_lanes(
    workers: &WorkerRuntime,
    lanes: StorageRuntimeLanes,
    executors: StorageExecutors,
    events: Arc<RuntimeEventPublisher>,
    clock_ms: fn() -> u64,
) -> Result<(), WorkerSpawnError> {
    let StorageRuntimeLanes {
        health,
        smart_observation,
        smart_control,
        directory_usage,
    } = lanes;
    let StorageExecutors {
        filesystem_health: mut execute_filesystem_health,
        smart_observation: mut execute_smart_observation,
        smart_control: mut execute_smart_control,
        directory_usage: execute_directory_usage,
    } = executors;

    spawn_observation_lane(
        workers,
        health,
        events.clone(),
        move |StorageHealthRequest::Refresh| execute_filesystem_health(clock_ms()),
        |snapshot| PlatformEvent::StorageHealth(StorageHealthEvent::Snapshot(snapshot)),
    )?;

    // Observation and mutation are isolated lanes but share one target-keyed,
    // generation-guarded state machine. Different disks coexist; an old poll
    // can never overwrite a restarted, canceled, removed, or expired job.
    let smart_state = Arc::new(SharedSmartRuntimeState::default());
    spawn_health_observation_lane(workers, smart_observation, events.clone(), {
        let smart_state = smart_state.clone();
        move |request| {
            smart_tracking::refresh(
                request,
                smart_state.as_ref(),
                execute_smart_observation.as_mut(),
                clock_ms(),
            )
        }
    })?;
    spawn_health_observation_lane(workers, smart_control, events.clone(), {
        let smart_state = smart_state.clone();
        move |request| {
            smart_tracking::control(
                request,
                smart_state.as_ref(),
                execute_smart_control.as_mut(),
                clock_ms(),
            )
        }
    })?;

    if let (Some(receiver), Some(executor)) = (directory_usage, execute_directory_usage) {
        spawn_directory_usage_lane(workers, receiver, events, executor, clock_ms)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/headless/storage.rs"]
mod tests;
