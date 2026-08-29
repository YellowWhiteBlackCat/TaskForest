//! OS-neutral power-supply execution with shared hotplug policy.

use std::sync::Arc;

use crossbeam_channel::Receiver;
use taskmanager_application::{PlatformEvent, PowerSupplyEvent, PowerSupplyRequest};
use taskmanager_core::{
    DEFAULT_DEVICE_ABSENCE_RETENTION_MS, DeviceRefreshOutcome, PowerSupplyLifecycleTracker,
    PowerSupplySnapshot,
};
use taskmanager_platform_contract::{CapabilityId, DeviceSourceSnapshot, ProviderFailure};

use crate::{
    Queued, RuntimeEventPublisher, WorkerRuntime, WorkerSpawnError, spawn_observation_lane,
};

type PowerSupplyExecutor = dyn FnMut(u64) -> Result<DeviceSourceSnapshot<PowerSupplySnapshot>, ProviderFailure>
    + Send
    + 'static;

/// Native power-supply observation adapted into an OS-independent closure.
pub struct PowerExecutors {
    supplies: Box<PowerSupplyExecutor>,
}

impl PowerExecutors {
    #[must_use]
    pub fn new<S>(supplies: S) -> Self
    where
        S: FnMut(u64) -> Result<DeviceSourceSnapshot<PowerSupplySnapshot>, ProviderFailure>
            + Send
            + 'static,
    {
        Self {
            supplies: Box::new(supplies),
        }
    }
}

/// Optional power-supply receiver while a native binding is assembled.
pub struct PendingPowerRuntimeLanes {
    pub supplies_rx: Option<Receiver<Queued<PowerSupplyRequest>>>,
}

impl PendingPowerRuntimeLanes {
    pub(crate) fn new(supplies_rx: Option<Receiver<Queued<PowerSupplyRequest>>>) -> Self {
        Self { supplies_rx }
    }

    pub(crate) fn missing_capabilities(&self) -> impl Iterator<Item = CapabilityId> {
        self.supplies_rx
            .is_none()
            .then_some(CapabilityId::POWER_SUPPLIES)
            .into_iter()
    }

    /// Promote power observation independently of every other hardware domain.
    #[must_use]
    pub fn try_complete(self) -> Option<PowerRuntimeLanes> {
        Some(PowerRuntimeLanes {
            supplies: self.supplies_rx?,
        })
    }
}

/// Complete provider-side receiver for power-supply observation.
pub struct PowerRuntimeLanes {
    supplies: Receiver<Queued<PowerSupplyRequest>>,
}

/// Attach power-supply observation to its independently bounded typed lane.
pub fn spawn_power_lanes(
    workers: &WorkerRuntime,
    lanes: PowerRuntimeLanes,
    executors: PowerExecutors,
    events: Arc<RuntimeEventPublisher>,
    clock_ms: fn() -> u64,
) -> Result<(), WorkerSpawnError> {
    let PowerRuntimeLanes { supplies } = lanes;
    let PowerExecutors {
        supplies: mut observe,
    } = executors;

    spawn_observation_lane(
        workers,
        supplies,
        events,
        {
            let mut lifecycles =
                PowerSupplyLifecycleTracker::new(DEFAULT_DEVICE_ABSENCE_RETENTION_MS);
            move |PowerSupplyRequest::Refresh| {
                let mut snapshot = observe(clock_ms())?;
                let outcome =
                    DeviceRefreshOutcome::from_discovery_outcome(snapshot.discovery().outcome);
                let discovered_devices = snapshot.discovered_devices().to_vec();
                lifecycles.reconcile_discovered(&mut snapshot.value, &discovered_devices, outcome);
                Ok(snapshot)
            }
        },
        |snapshot| PlatformEvent::PowerSupplies(PowerSupplyEvent::Snapshot(snapshot)),
    )
}

#[cfg(test)]
#[path = "../tests/headless/power.rs"]
mod tests;
