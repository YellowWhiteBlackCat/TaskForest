//! Complete-runtime typestate: promotion from optional provider bindings to a
//! total standard product surface.
//!
//! `try_complete` fails closed with every missing capability when a native
//! adapter requests the full surface but its bindings and lanes have drifted
//! apart.

use std::sync::Arc;
use std::{error, fmt};

use taskmanager_application::PlatformHandle;
use taskmanager_platform_contract::CapabilityId;

use crate::channel::{ChannelRuntime, RuntimeLanes};
use crate::delivery::LaneStartRegistry;
use crate::delivery::RuntimeEventPublisher;
use crate::environment::EnvironmentRuntimeLanes;
use crate::integration::IntegrationRuntimeLanes;
use crate::power::PowerRuntimeLanes;
use crate::process::ProcessRuntimeLanes;
use crate::sensor::SensorRuntimeLanes;
use crate::service::ServiceRuntimeLanes;
use crate::storage::StorageRuntimeLanes;
use crate::system::SystemRuntimeLanes;
use crate::{RuntimeConstructionError, WorkerSpawnError};

/// A native adapter requested complete composition but omitted one or more
/// required capability bindings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositionError {
    missing_capabilities: Vec<CapabilityId>,
    worker_spawn: Option<WorkerSpawnError>,
    runtime_construction: Option<RuntimeConstructionError>,
}

impl CompositionError {
    #[must_use]
    pub fn missing_capabilities(&self) -> &[CapabilityId] {
        &self.missing_capabilities
    }

    /// Worker startup failure after capability composition succeeded.
    #[must_use]
    pub fn worker_spawn_error(&self) -> Option<&WorkerSpawnError> {
        self.worker_spawn.as_ref()
    }

    #[must_use]
    pub fn runtime_construction_error(&self) -> Option<&RuntimeConstructionError> {
        self.runtime_construction.as_ref()
    }

    #[must_use]
    pub fn worker_spawn(error: WorkerSpawnError) -> Self {
        Self {
            missing_capabilities: Vec::new(),
            worker_spawn: Some(error),
            runtime_construction: None,
        }
    }

    #[must_use]
    pub fn runtime_construction(error: RuntimeConstructionError) -> Self {
        Self {
            missing_capabilities: Vec::new(),
            worker_spawn: None,
            runtime_construction: Some(error),
        }
    }
}

impl fmt::Display for CompositionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(error) = &self.worker_spawn {
            return write!(formatter, "native runtime worker startup failed: {error}");
        }
        if let Some(error) = &self.runtime_construction {
            return write!(formatter, "native runtime construction failed: {error}");
        }
        formatter.write_str("incomplete native runtime; missing capabilities: ")?;
        for (index, capability) in self.missing_capabilities.iter().enumerate() {
            if index > 0 {
                formatter.write_str(", ")?;
            }
            fmt::Display::fmt(capability, formatter)?;
        }
        Ok(())
    }
}

impl error::Error for CompositionError {}

/// Provider-side receivers after a complete native adapter has proved that
/// every shared product capability is bound.
pub struct CompleteRuntimeLanes {
    pub system: SystemRuntimeLanes,
    pub process: ProcessRuntimeLanes,
    pub service: ServiceRuntimeLanes,
    pub environment: EnvironmentRuntimeLanes,
    pub integration: IntegrationRuntimeLanes,
    pub storage: StorageRuntimeLanes,
    pub sensor: SensorRuntimeLanes,
    pub power: PowerRuntimeLanes,
}

impl RuntimeLanes {
    fn missing_capabilities(&self) -> Vec<CapabilityId> {
        let mut missing_capabilities: Vec<_> = self.system.missing_capabilities().collect();
        missing_capabilities.extend(self.process.missing_capabilities());
        missing_capabilities.extend(self.service.missing_capabilities());
        missing_capabilities.extend(self.environment.missing_capabilities());
        missing_capabilities.extend(self.integration.missing_capabilities());
        if self.storage.health_capability_missing() {
            missing_capabilities.push(CapabilityId::STORAGE_HEALTH);
        }
        missing_capabilities.extend(self.sensor.missing_capabilities());
        missing_capabilities.extend(self.power.missing_capabilities());
        missing_capabilities.extend(self.storage.missing_smart_capabilities());
        missing_capabilities
    }

    /// Convert an optional capability set into the complete standard product
    /// surface or return every missing capability.
    pub fn try_complete(self) -> Result<CompleteRuntimeLanes, CompositionError> {
        let missing_capabilities = self.missing_capabilities();
        let Self {
            system,
            process,
            service,
            environment,
            integration,
            storage,
            sensor,
            power,
        } = self;
        let Some(system) = system.try_complete() else {
            return Err(CompositionError {
                missing_capabilities,
                worker_spawn: None,
                runtime_construction: None,
            });
        };
        let Some(process) = process.try_complete() else {
            return Err(CompositionError {
                missing_capabilities,
                worker_spawn: None,
                runtime_construction: None,
            });
        };
        let Some(service) = service.try_complete() else {
            return Err(CompositionError {
                missing_capabilities,
                worker_spawn: None,
                runtime_construction: None,
            });
        };
        let Some(environment) = environment.try_complete() else {
            return Err(CompositionError {
                missing_capabilities,
                worker_spawn: None,
                runtime_construction: None,
            });
        };
        let Some(integration) = integration.try_complete() else {
            return Err(CompositionError {
                missing_capabilities,
                worker_spawn: None,
                runtime_construction: None,
            });
        };
        let Some(storage) = storage.try_complete() else {
            return Err(CompositionError {
                missing_capabilities,
                worker_spawn: None,
                runtime_construction: None,
            });
        };
        let Some(sensor) = sensor.try_complete() else {
            return Err(CompositionError {
                missing_capabilities,
                worker_spawn: None,
                runtime_construction: None,
            });
        };
        let Some(power) = power.try_complete() else {
            return Err(CompositionError {
                missing_capabilities,
                worker_spawn: None,
                runtime_construction: None,
            });
        };
        Ok(CompleteRuntimeLanes {
            system,
            process,
            service,
            environment,
            integration,
            storage,
            sensor,
            power,
        })
    }
}

/// Runtime ownership returned only after every standard product capability has
/// a provider binding and typed lane.
pub struct CompleteChannelRuntime {
    pub handle: PlatformHandle,
    pub publisher: Arc<RuntimeEventPublisher>,
    pub lanes: CompleteRuntimeLanes,
    pub(crate) lane_starters: Arc<LaneStartRegistry>,
}

impl ChannelRuntime {
    /// Fail closed when a native adapter claims the complete standard product
    /// surface but its bindings and lanes have drifted apart.
    pub fn try_complete(self) -> Result<CompleteChannelRuntime, CompositionError> {
        let Self {
            handle,
            publisher,
            lane_starters,
            lanes,
        } = self;
        let lanes = lanes.try_complete()?;
        Ok(CompleteChannelRuntime {
            handle,
            publisher,
            lanes,
            lane_starters,
        })
    }
}

#[cfg(test)]
#[path = "../tests/headless/runtime_composition_tests.rs"]
mod tests;
