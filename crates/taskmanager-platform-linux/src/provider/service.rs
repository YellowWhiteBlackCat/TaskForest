//! Linux service inventory, control, dependency, and log providers.

use taskmanager_core::{
    ServiceAction, ServiceDeps, ServiceId, ServiceItem, ServiceLogQuery, ServiceLogState,
    ServiceLogStreamState,
};
use taskmanager_platform_contract::{PartialSourceSnapshot, ProviderFailure};
use taskmanager_platform_provider::{
    ServiceControlProvider, ServiceDependenciesProvider, ServiceInventoryProvider,
    ServiceLogSnapshotProvider, ServiceLogStreamProvider,
};

use crate::engine::services::ServiceManager;

pub(super) struct NativeServiceInventoryProvider;

impl ServiceInventoryProvider for NativeServiceInventoryProvider {
    fn refresh(&mut self) -> Result<PartialSourceSnapshot<ServiceItem>, ProviderFailure> {
        #[cfg(target_os = "linux")]
        {
            Ok(ServiceManager::scan_snapshot())
        }
        #[cfg(not(target_os = "linux"))]
        {
            Err(ProviderFailure::TemporarilyUnavailable)
        }
    }
}

pub(super) struct NativeServiceDependenciesProvider;

impl ServiceDependenciesProvider for NativeServiceDependenciesProvider {
    fn dependencies(&mut self, service_id: &ServiceId) -> Result<ServiceDeps, ProviderFailure> {
        ServiceManager::fetch_deps(service_id)
    }
}

pub(super) struct NativeServiceControlProvider;

impl ServiceControlProvider for NativeServiceControlProvider {
    fn control(
        &mut self,
        service_id: &ServiceId,
        action: ServiceAction,
    ) -> Result<(), ProviderFailure> {
        ServiceManager::control_service(service_id, action)
    }
}

pub(super) struct NativeServiceLogSnapshotProvider;

impl ServiceLogSnapshotProvider for NativeServiceLogSnapshotProvider {
    fn snapshot(&mut self, service_id: &ServiceId) -> Result<ServiceLogState, ProviderFailure> {
        ServiceManager::fetch_logs(service_id)
    }
}

pub(super) struct NativeServiceLogStreamProvider;

impl ServiceLogStreamProvider for NativeServiceLogStreamProvider {
    fn stream(
        &mut self,
        query: &ServiceLogQuery,
        observed_at_ms: u64,
    ) -> Result<ServiceLogStreamState, ProviderFailure> {
        ServiceManager::fetch_log_stream(query, observed_at_ms)
    }
}
