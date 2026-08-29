//! OS-neutral sensor execution with shared hotplug policy.

use std::sync::Arc;

use crossbeam_channel::Receiver;
use taskmanager_application::{PlatformEvent, SensorEvent, SensorRequest};
use taskmanager_core::{
    DEFAULT_DEVICE_ABSENCE_RETENTION_MS, DeviceRefreshOutcome, SensorCenterSnapshot,
    SensorLifecycleTracker,
};
use taskmanager_platform_contract::{CapabilityId, DeviceSourceSnapshot, ProviderFailure};

use crate::{
    Queued, RuntimeEventPublisher, WorkerRuntime, WorkerSpawnError, spawn_lazy_observation_lane,
};

type SensorExecutor = dyn FnMut(u64) -> Result<DeviceSourceSnapshot<SensorCenterSnapshot>, ProviderFailure>
    + Send
    + 'static;

/// Native sensor observation adapted into an OS-independent closure.
pub struct SensorExecutors {
    observation: Box<SensorExecutor>,
}

impl SensorExecutors {
    #[must_use]
    pub fn new<O>(observation: O) -> Self
    where
        O: FnMut(u64) -> Result<DeviceSourceSnapshot<SensorCenterSnapshot>, ProviderFailure>
            + Send
            + 'static,
    {
        Self {
            observation: Box::new(observation),
        }
    }
}

/// Optional sensor receiver while a native binding is assembled.
pub struct PendingSensorRuntimeLanes {
    pub observation_rx: Option<Receiver<Queued<SensorRequest>>>,
}

impl PendingSensorRuntimeLanes {
    pub(crate) fn new(observation_rx: Option<Receiver<Queued<SensorRequest>>>) -> Self {
        Self { observation_rx }
    }

    pub(crate) fn missing_capabilities(&self) -> impl Iterator<Item = CapabilityId> {
        self.observation_rx
            .is_none()
            .then_some(CapabilityId::SENSORS)
            .into_iter()
    }

    /// Promote sensor observation independently of every other hardware domain.
    #[must_use]
    pub fn try_complete(self) -> Option<SensorRuntimeLanes> {
        Some(SensorRuntimeLanes {
            observation: self.observation_rx?,
        })
    }
}

/// Complete provider-side receiver for sensor observation.
pub struct SensorRuntimeLanes {
    observation: Receiver<Queued<SensorRequest>>,
}

/// Attach sensor observation to its independently bounded typed lane.
pub fn spawn_sensor_lanes(
    workers: &WorkerRuntime,
    lanes: SensorRuntimeLanes,
    executors: SensorExecutors,
    events: Arc<RuntimeEventPublisher>,
    clock_ms: fn() -> u64,
) -> Result<(), WorkerSpawnError> {
    let SensorRuntimeLanes { observation } = lanes;
    let SensorExecutors {
        observation: mut observe,
    } = executors;

    spawn_lazy_observation_lane(
        workers,
        observation,
        events,
        {
            let mut lifecycles = SensorLifecycleTracker::new(DEFAULT_DEVICE_ABSENCE_RETENTION_MS);
            move |SensorRequest::Refresh| {
                let mut snapshot = observe(clock_ms())?;
                let outcome =
                    DeviceRefreshOutcome::from_discovery_outcome(snapshot.discovery().outcome);
                let discovered_devices = snapshot.discovered_devices().to_vec();
                lifecycles.reconcile_discovered(&mut snapshot.value, &discovered_devices, outcome);
                Ok(snapshot)
            }
        },
        |snapshot| PlatformEvent::Sensors(SensorEvent::Snapshot(snapshot)),
    )
}

#[cfg(test)]
#[path = "../tests/headless/sensor.rs"]
mod tests;
