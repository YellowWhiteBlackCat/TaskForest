use taskmanager_application::{DirectoryUsageRequest, automatic_schedules};
use taskmanager_core::core::identity::ProviderId;
use taskmanager_platform_contract::CapabilityRequest;

use crate::{ProviderBinding, RuntimeConfig, RuntimeProviderBindings};

use super::*;

fn fixed_clock() -> u64 {
    11
}

fn complete_bindings() -> RuntimeProviderBindings {
    let provider = ProviderId::borrowed("fixture.complete");
    let mut bindings = RuntimeProviderBindings::default();
    bindings.system.host = ProviderBinding::present(provider.clone());
    bindings.system.cpu = ProviderBinding::present(provider.clone());
    bindings.system.memory = ProviderBinding::present(provider.clone());
    bindings.system.storage = ProviderBinding::present(provider.clone());
    bindings.system.network = ProviderBinding::present(provider.clone());
    bindings.system.gpu = ProviderBinding::present(provider.clone());
    bindings.system.hardware_inventory = ProviderBinding::present(provider.clone());
    bindings.system.containers = ProviderBinding::present(provider.clone());
    bindings.system.npu_inventory = ProviderBinding::present(provider.clone());
    bindings.process.list = ProviderBinding::present(provider.clone());
    bindings.process.control = ProviderBinding::present(provider.clone());
    bindings.process.network = ProviderBinding::present(provider.clone());
    bindings.process.gpu = ProviderBinding::present(provider.clone());
    bindings.process.resources = ProviderBinding::present(provider.clone());
    bindings.process.isolation = ProviderBinding::present(provider.clone());
    bindings.process.threads = ProviderBinding::present(provider.clone());
    bindings.process.affinity = ProviderBinding::present(provider.clone());
    bindings.process.affinity_control = ProviderBinding::present(provider.clone());
    bindings.process.resource_control = ProviderBinding::present(provider.clone());
    bindings.process.network_escalation = ProviderBinding::present(provider.clone());
    bindings.service.inventory = ProviderBinding::present(provider.clone());
    bindings.service.dependencies = ProviderBinding::present(provider.clone());
    bindings.service.control = ProviderBinding::present(provider.clone());
    bindings.service.log_snapshot = ProviderBinding::present(provider.clone());
    bindings.service.log_stream = ProviderBinding::present(provider.clone());
    bindings.environment.startup_inventory = ProviderBinding::present(provider.clone());
    bindings.environment.startup_evidence = ProviderBinding::present(provider.clone());
    bindings.environment.startup_control = ProviderBinding::present(provider.clone());
    bindings.environment.session_inventory = ProviderBinding::present(provider.clone());
    bindings.environment.session_control = ProviderBinding::present(provider.clone());
    bindings.integration.command_launch = ProviderBinding::present(provider.clone());
    bindings.integration.resource_reveal = ProviderBinding::present(provider.clone());
    bindings.integration.url_open = ProviderBinding::present(provider.clone());
    bindings.integration.desktop_appearance = ProviderBinding::present(provider.clone());
    bindings.integration.desktop_notification = ProviderBinding::present(provider.clone());
    bindings.storage.health = ProviderBinding::present(provider.clone());
    bindings.storage.smart_observation = ProviderBinding::present(provider.clone());
    bindings.storage.smart_control = ProviderBinding::present(provider.clone());
    bindings.sensor.observation = ProviderBinding::present(provider.clone());
    bindings.power.supplies = ProviderBinding::present(provider.clone());
    bindings
}

#[test]
fn complete_conversion_reports_every_missing_capability() {
    let runtime = ChannelRuntime::new(
        RuntimeProviderBindings::default(),
        RuntimeConfig::new(fixed_clock),
    );

    let error = runtime
        .try_complete()
        .err()
        .expect("empty bindings must fail complete composition");

    assert_eq!(
        error.missing_capabilities(),
        [
            CapabilityId::TELEMETRY_HOST,
            CapabilityId::TELEMETRY_CPU,
            CapabilityId::TELEMETRY_MEMORY,
            CapabilityId::TELEMETRY_STORAGE,
            CapabilityId::TELEMETRY_NETWORK,
            CapabilityId::TELEMETRY_GPU,
            CapabilityId::CONTAINERS,
            CapabilityId::HARDWARE_INVENTORY,
            CapabilityId::PROCESS_LIST,
            CapabilityId::PROCESS_INSIGHTS_NETWORK,
            CapabilityId::PROCESS_INSIGHTS_GPU,
            CapabilityId::PROCESS_INSIGHTS_RESOURCES,
            CapabilityId::PROCESS_INSIGHTS_ISOLATION,
            CapabilityId::PROCESS_INSIGHTS_THREADS,
            CapabilityId::PROCESS_AFFINITY,
            CapabilityId::PROCESS_AFFINITY_CONTROL,
            CapabilityId::PROCESS_RESOURCE_CONTROL,
            CapabilityId::PROCESS_NETWORK_ESCALATION,
            CapabilityId::PROCESS_CONTROL,
            CapabilityId::SERVICES,
            CapabilityId::SERVICE_DEPENDENCIES,
            CapabilityId::SERVICE_CONTROL,
            CapabilityId::SERVICE_LOGS,
            CapabilityId::SERVICE_LOG_STREAM,
            CapabilityId::STARTUP,
            CapabilityId::STARTUP_EVIDENCE,
            CapabilityId::STARTUP_CONTROL,
            CapabilityId::SESSIONS,
            CapabilityId::SESSION_CONTROL,
            CapabilityId::COMMAND_LAUNCH,
            CapabilityId::RESOURCE_REVEAL,
            CapabilityId::URL_OPEN,
            CapabilityId::DESKTOP_APPEARANCE,
            CapabilityId::STORAGE_HEALTH,
            CapabilityId::SENSORS,
            CapabilityId::POWER_SUPPLIES,
            CapabilityId::SMART,
            CapabilityId::SMART_CONTROL,
        ],
        "typestate grouping must preserve the public capability validation order"
    );
}

#[test]
fn complete_conversion_returns_non_optional_typed_lanes() {
    let complete = ChannelRuntime::new(complete_bindings(), RuntimeConfig::new(fixed_clock))
        .try_complete()
        .expect("complete bindings");

    assert!(complete.handle.host_telemetry().is_some());
    assert!(complete.handle.cpu_telemetry().is_some());
    assert!(complete.handle.memory_telemetry().is_some());
    assert!(complete.handle.storage_telemetry().is_some());
    assert!(complete.handle.network_telemetry().is_some());
    assert!(complete.handle.gpu_telemetry().is_some());
    assert!(complete.handle.smart_control().is_some());
}

#[test]
fn nested_process_group_preserves_per_capability_completion_errors() {
    let mut bindings = complete_bindings();
    bindings.process.affinity_control = ProviderBinding::absent();

    let error = ChannelRuntime::new(bindings, RuntimeConfig::new(fixed_clock))
        .try_complete()
        .err()
        .expect("one missing process binding must reject complete composition");

    assert_eq!(
        error.missing_capabilities(),
        [CapabilityId::PROCESS_AFFINITY_CONTROL],
        "group promotion must not collapse the process capabilities into one error"
    );
}

#[test]
fn nested_system_and_integration_groups_preserve_per_capability_errors() {
    let mut system_bindings = complete_bindings();
    system_bindings.system.hardware_inventory = ProviderBinding::absent();
    let system_error = ChannelRuntime::new(system_bindings, RuntimeConfig::new(fixed_clock))
        .try_complete()
        .err()
        .expect("one missing system binding must reject complete composition");
    assert_eq!(
        system_error.missing_capabilities(),
        [CapabilityId::HARDWARE_INVENTORY]
    );

    let mut integration_bindings = complete_bindings();
    integration_bindings.integration.url_open = ProviderBinding::absent();
    let integration_error =
        ChannelRuntime::new(integration_bindings, RuntimeConfig::new(fixed_clock))
            .try_complete()
            .err()
            .expect("one missing integration binding must reject complete composition");
    assert_eq!(
        integration_error.missing_capabilities(),
        [CapabilityId::URL_OPEN]
    );
}

#[test]
fn environment_sensor_and_power_groups_preserve_capability_errors() {
    let mut environment_bindings = complete_bindings();
    environment_bindings.environment.startup_control = ProviderBinding::absent();
    let environment_error =
        ChannelRuntime::new(environment_bindings, RuntimeConfig::new(fixed_clock))
            .try_complete()
            .err()
            .expect("one missing environment binding must reject complete composition");
    assert_eq!(
        environment_error.missing_capabilities(),
        [CapabilityId::STARTUP_CONTROL]
    );

    let mut sensor_bindings = complete_bindings();
    sensor_bindings.sensor.observation = ProviderBinding::absent();
    let sensor_error = ChannelRuntime::new(sensor_bindings, RuntimeConfig::new(fixed_clock))
        .try_complete()
        .err()
        .expect("one missing sensor binding must reject complete composition");
    assert_eq!(sensor_error.missing_capabilities(), [CapabilityId::SENSORS]);

    let mut power_bindings = complete_bindings();
    power_bindings.power.supplies = ProviderBinding::absent();
    let power_error = ChannelRuntime::new(power_bindings, RuntimeConfig::new(fixed_clock))
        .try_complete()
        .err()
        .expect("one missing power binding must reject complete composition");
    assert_eq!(
        power_error.missing_capabilities(),
        [CapabilityId::POWER_SUPPLIES]
    );
}

#[test]
fn storage_group_preserves_each_capability_and_global_validation_order() {
    let mut one_missing = complete_bindings();
    one_missing.storage.smart_observation = ProviderBinding::absent();
    let one_error = ChannelRuntime::new(one_missing, RuntimeConfig::new(fixed_clock))
        .try_complete()
        .err()
        .expect("one missing storage binding must reject complete composition");
    assert_eq!(one_error.missing_capabilities(), [CapabilityId::SMART]);

    let mut all_missing = complete_bindings();
    all_missing.storage.health = ProviderBinding::absent();
    all_missing.storage.smart_observation = ProviderBinding::absent();
    all_missing.storage.smart_control = ProviderBinding::absent();
    let all_error = ChannelRuntime::new(all_missing, RuntimeConfig::new(fixed_clock))
        .try_complete()
        .err()
        .expect("missing storage group must reject complete composition");
    assert_eq!(
        all_error.missing_capabilities(),
        [
            CapabilityId::STORAGE_HEALTH,
            CapabilityId::SMART,
            CapabilityId::SMART_CONTROL,
        ]
    );
}

#[test]
fn complete_routes_are_total_and_catalog_attribution_is_unique() {
    let routes = complete_bindings().routes();
    let mut route_capabilities = std::collections::BTreeSet::new();
    for route in &routes {
        assert!(
            route_capabilities.insert(route.capability.clone()),
            "duplicate capability route {}",
            route.capability
        );
        assert!(!route.provider.as_str().is_empty());
    }

    let automatic_entries = automatic_schedules().collect::<Vec<_>>();
    let automatic = automatic_entries
        .iter()
        .cloned()
        .map(|schedule| (schedule.capability, schedule.cadence_ms))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(automatic.len(), automatic_entries.len());
    for route in &routes {
        assert_eq!(
            route.cadence_ms,
            automatic.get(&route.capability).copied(),
            "route cadence must be derived from the closed application registry"
        );
    }
    for capability in automatic.keys() {
        assert!(
            route_capabilities.contains(capability),
            "every automatic dispatcher entry needs a real typed runtime route"
        );
    }

    let runtime = ChannelRuntime::new(complete_bindings(), RuntimeConfig::new(fixed_clock));
    let snapshot = runtime.handle.capabilities().snapshot();
    assert_eq!(
        snapshot.iter().count(),
        routes.len(),
        "catalog descriptors must match the typed route table"
    );
    let mut catalog_capabilities = std::collections::BTreeSet::new();
    for descriptor in snapshot.iter() {
        assert!(
            catalog_capabilities.insert(descriptor.id.clone()),
            "duplicate catalog descriptor {}",
            descriptor.id
        );
        assert_eq!(
            descriptor.providers.len(),
            1,
            "each capability must be attributed to exactly one provider"
        );
    }
}

#[test]
fn optional_directory_usage_facet_adds_one_route_catalog_entry_and_port() {
    let mut bindings = complete_bindings();
    let baseline_routes = bindings.routes().len();
    let provider = ProviderId::borrowed("fixture.directory-usage");
    let registration =
        crate::ProviderRegistration::<DirectoryUsageRequest, _>::new(provider.clone(), ());
    bindings.storage = bindings.storage.with_directory_usage(&registration);
    assert_eq!(
        bindings.routes().len(),
        baseline_routes + 1,
        "the optional facet adds exactly one typed route"
    );
    assert_eq!(
        bindings
            .routes()
            .into_iter()
            .find(|route| route.capability == DirectoryUsageRequest::CAPABILITY)
            .map(|route| route.sideband_policy),
        Some(DirectoryUsageRequest::SIDEBAND_POLICY)
    );

    let complete = ChannelRuntime::new(bindings, RuntimeConfig::new(fixed_clock))
        .try_complete()
        .expect("the optional directory-usage facet must not block complete composition");
    assert!(
        complete
            .handle
            .capabilities()
            .snapshot()
            .get(&CapabilityId::DIRECTORY_USAGE)
            .is_some(),
        "the optional facet must add its catalog descriptor"
    );
    assert!(
        complete
            .handle
            .facets()
            .storage()
            .directory_usage()
            .is_some(),
        "the optional facet must create its request port"
    );
    assert_eq!(
        complete
            .handle
            .capabilities()
            .snapshot()
            .get(&CapabilityId::DIRECTORY_USAGE)
            .map(|descriptor| descriptor.providers.as_slice()),
        Some([provider].as_slice()),
        "the catalog descriptor must attribute the facet to its provider"
    );
}
