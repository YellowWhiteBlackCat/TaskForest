use taskmanager_platform_contract::{CapabilityId, CapabilityRequest};

use super::*;
use crate::{SessionControlRequest, StartupControlRequest};

fn assert_capability<R: CapabilityRequest>(expected: CapabilityId) {
    assert_eq!(R::CAPABILITY, expected);
}

#[test]
fn every_typed_request_owns_exactly_one_platform_neutral_capability() {
    assert_capability::<HostTelemetryRequest>(CapabilityId::TELEMETRY_HOST);
    assert_capability::<CpuTelemetryRequest>(CapabilityId::TELEMETRY_CPU);
    assert_capability::<MemoryTelemetryRequest>(CapabilityId::TELEMETRY_MEMORY);
    assert_capability::<StorageTelemetryRequest>(CapabilityId::TELEMETRY_STORAGE);
    assert_capability::<NetworkTelemetryRequest>(CapabilityId::TELEMETRY_NETWORK);
    assert_capability::<GpuTelemetryRequest>(CapabilityId::TELEMETRY_GPU);
    assert_capability::<HardwareInventoryRequest>(CapabilityId::HARDWARE_INVENTORY);
    assert_capability::<ContainerRollupRequest>(CapabilityId::CONTAINERS);
    assert_capability::<ProcessListRequest>(CapabilityId::PROCESS_LIST);
    assert_capability::<ProcessControlRequest>(CapabilityId::PROCESS_CONTROL);
    assert_capability::<ProcessNetworkRequest>(CapabilityId::PROCESS_INSIGHTS_NETWORK);
    assert_capability::<ProcessGpuRequest>(CapabilityId::PROCESS_INSIGHTS_GPU);
    assert_capability::<ProcessResourcesRequest>(CapabilityId::PROCESS_INSIGHTS_RESOURCES);
    assert_capability::<ProcessIsolationRequest>(CapabilityId::PROCESS_INSIGHTS_ISOLATION);
    assert_capability::<ProcessThreadsRequest>(CapabilityId::PROCESS_INSIGHTS_THREADS);
    assert_capability::<ProcessEnvironmentRequest>(CapabilityId::PROCESS_INSIGHTS_ENVIRONMENT);
    assert_capability::<ProcessAffinityRequest>(CapabilityId::PROCESS_AFFINITY);
    assert_capability::<ProcessAffinityControlRequest>(CapabilityId::PROCESS_AFFINITY_CONTROL);
    assert_capability::<ProcessResourceControlRequest>(CapabilityId::PROCESS_RESOURCE_CONTROL);
    assert_capability::<ProcessNetworkEscalationRequest>(CapabilityId::PROCESS_NETWORK_ESCALATION);
    assert_capability::<ServiceInventoryRequest>(CapabilityId::SERVICES);
    assert_capability::<ServiceDependenciesRequest>(CapabilityId::SERVICE_DEPENDENCIES);
    assert_capability::<ServiceControlRequest>(CapabilityId::SERVICE_CONTROL);
    assert_capability::<ServiceLogSnapshotRequest>(CapabilityId::SERVICE_LOGS);
    assert_capability::<ServiceLogStreamRequest>(CapabilityId::SERVICE_LOG_STREAM);
    assert_capability::<StartupInventoryRequest>(CapabilityId::STARTUP);
    assert_capability::<StartupEvidenceRequest>(CapabilityId::STARTUP_EVIDENCE);
    assert_capability::<StartupControlRequest>(CapabilityId::STARTUP_CONTROL);
    assert_capability::<SessionInventoryRequest>(CapabilityId::SESSIONS);
    assert_capability::<SessionControlRequest>(CapabilityId::SESSION_CONTROL);
    assert_capability::<CommandLaunchRequest>(CapabilityId::COMMAND_LAUNCH);
    assert_capability::<ResourceRevealRequest>(CapabilityId::RESOURCE_REVEAL);
    assert_capability::<UrlOpenRequest>(CapabilityId::URL_OPEN);
    assert_capability::<DesktopAppearanceRequest>(CapabilityId::DESKTOP_APPEARANCE);
    assert_capability::<SetupScriptRequest>(CapabilityId::FIRST_RUN_SETUP);
    assert_capability::<StorageHealthRequest>(CapabilityId::STORAGE_HEALTH);
    assert_capability::<SmartObservationRequest>(CapabilityId::SMART);
    assert_capability::<SmartControlRequest>(CapabilityId::SMART_CONTROL);
    assert_capability::<SensorRequest>(CapabilityId::SENSORS);
    assert_capability::<PowerSupplyRequest>(CapabilityId::POWER_SUPPLIES);
    assert_capability::<DirectoryUsageRequest>(CapabilityId::DIRECTORY_USAGE);
    assert_capability::<GpuEngineRowsRequest>(CapabilityId::TELEMETRY_GPU_ENGINES);
    assert_capability::<NpuInventoryRequest>(CapabilityId::ACCELERATOR_NPU);
}
