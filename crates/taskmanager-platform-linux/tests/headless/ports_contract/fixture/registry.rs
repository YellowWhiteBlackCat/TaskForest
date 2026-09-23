//! Registry composition: binds every standard capability to the shared
//! fixture provider under its canonical `fixture.*` identity.

use super::*;

pub fn fake_registry(provider: FakeProvider) -> LinuxProviderRegistry {
    LinuxProviderRegistry::new(LinuxProviderRegistryParams {
        system: SystemProviders::new(
            SystemObservationProviders::new(
                ProviderRegistration::new(
                    ProviderId::borrowed("fixture.system.host"),
                    provider.clone(),
                ),
                ProviderRegistration::new(
                    ProviderId::borrowed("fixture.system.cpu"),
                    provider.clone(),
                ),
                ProviderRegistration::new(
                    ProviderId::borrowed("fixture.system.memory"),
                    provider.clone(),
                ),
                ProviderRegistration::new(
                    ProviderId::borrowed("fixture.system.storage"),
                    provider.clone(),
                ),
                ProviderRegistration::new(
                    ProviderId::borrowed("fixture.system.network"),
                    provider.clone(),
                ),
                ProviderRegistration::new(
                    ProviderId::borrowed("fixture.system.gpu"),
                    provider.clone(),
                ),
                ProviderRegistration::new(
                    ProviderId::borrowed("fixture.system.containers"),
                    provider.clone(),
                ),
            ),
            SystemAuxiliaryProviders::new(SystemAuxiliaryProvidersParams {
                hardware_inventory: ProviderRegistration::new(
                    ProviderId::borrowed("fixture.system.hardware-inventory"),
                    Box::new(provider.clone()) as Box<dyn HardwareInventoryProvider>,
                ),
                gpu_engine_rows: ProviderRegistration::new(
                    ProviderId::borrowed("fixture.system.gpu-engine-rows"),
                    Box::new(provider.clone()) as Box<dyn GpuEngineRowsProvider>,
                ),
                npu_inventory: ProviderRegistration::new(
                    ProviderId::borrowed("fixture.system.npu-inventory"),
                    Box::new(provider.clone()) as Box<dyn NpuInventoryProvider>,
                ),
                smbios_memory: ProviderRegistration::new(
                    ProviderId::borrowed("fixture.system.smbios-memory"),
                    Box::new(provider.clone()) as Box<dyn SmbiosMemoryProvider>,
                ),
                rapl_power: ProviderRegistration::new(
                    ProviderId::borrowed("fixture.system.rapl-power"),
                    Box::new(provider.clone()) as Box<dyn RaplPowerProvider>,
                ),
                msr_readout: ProviderRegistration::new(
                    ProviderId::borrowed("fixture.system.msr-readout"),
                    Box::new(provider.clone()) as Box<dyn MsrReadoutProvider>,
                ),
                cpu_throttle: ProviderRegistration::new(
                    ProviderId::borrowed("fixture.system.cpu-throttle"),
                    Box::new(provider.clone()) as Box<dyn CpuThrottleProvider>,
                ),
            }),
        ),
        processes: ProcessProviders::new(
            ProcessObservationProviders::new(ProcessObservationProvidersParams {
                list: ProviderRegistration::new(
                    ProviderId::borrowed("fixture.process.list"),
                    Box::new(provider.clone()) as Box<dyn ProcessListProvider>,
                ),
                network: ProviderRegistration::new(
                    ProviderId::borrowed("fixture.process.network"),
                    Box::new(provider.clone()) as Box<dyn ProcessNetworkProvider>,
                ),
                gpu: ProviderRegistration::new(
                    ProviderId::borrowed("fixture.process.gpu"),
                    Box::new(provider.clone()) as Box<dyn ProcessGpuProvider>,
                ),
                resources: ProviderRegistration::new(
                    ProviderId::borrowed("fixture.process.resources"),
                    Box::new(provider.clone()) as Box<dyn ProcessResourcesProvider>,
                ),
                isolation: ProviderRegistration::new(
                    ProviderId::borrowed("fixture.process.isolation"),
                    Box::new(provider.clone()) as Box<dyn ProcessIsolationProvider>,
                ),
                threads: ProviderRegistration::new(
                    ProviderId::borrowed("fixture.process.threads"),
                    Box::new(provider.clone()) as Box<dyn ProcessThreadsProvider>,
                ),
                open_files: ProviderRegistration::new(
                    ProviderId::borrowed("fixture.process.open_files"),
                    Box::new(provider.clone()) as Box<dyn ProcessOpenFilesProvider>,
                ),
                environment: ProviderRegistration::new(
                    ProviderId::borrowed("fixture.process.environment"),
                    Box::new(provider.clone()) as Box<dyn ProcessEnvironmentProvider>,
                ),
                affinity: ProviderRegistration::new(
                    ProviderId::borrowed("fixture.process.affinity"),
                    Box::new(provider.clone()) as Box<dyn ProcessAffinityProvider>,
                ),
            }),
            ProcessControlProviders::new(
                ProviderRegistration::new(
                    ProviderId::borrowed("fixture.process.affinity-control"),
                    provider.clone(),
                ),
                ProviderRegistration::new(
                    ProviderId::borrowed("fixture.process.control"),
                    provider.clone(),
                ),
                ProviderRegistration::new(
                    ProviderId::borrowed("fixture.process.resource-control"),
                    provider.clone(),
                ),
                ProviderRegistration::new(
                    ProviderId::borrowed("fixture.process.network-escalation"),
                    provider.clone(),
                ),
            ),
        ),
        services: ServiceProviders::new(
            ProviderRegistration::new(
                ProviderId::borrowed("fixture.service.inventory"),
                provider.clone(),
            ),
            ProviderRegistration::new(
                ProviderId::borrowed("fixture.service.dependencies"),
                provider.clone(),
            ),
            ProviderRegistration::new(
                ProviderId::borrowed("fixture.service.control"),
                provider.clone(),
            ),
            ProviderRegistration::new(
                ProviderId::borrowed("fixture.service.log-snapshot"),
                provider.clone(),
            ),
            ProviderRegistration::new(
                ProviderId::borrowed("fixture.service.log-stream"),
                provider.clone(),
            ),
        ),
        environment: EnvironmentProviders::new(
            ProviderRegistration::new(
                ProviderId::borrowed("fixture.environment.startup-inventory"),
                provider.clone(),
            ),
            ProviderRegistration::new(
                ProviderId::borrowed("fixture.environment.startup-evidence"),
                provider.clone(),
            ),
            ProviderRegistration::new(
                ProviderId::borrowed("fixture.environment.startup-control"),
                provider.clone(),
            ),
            ProviderRegistration::new(
                ProviderId::borrowed("fixture.environment.session-inventory"),
                provider.clone(),
            ),
            ProviderRegistration::new(
                ProviderId::borrowed("fixture.environment.session-control"),
                provider.clone(),
            ),
        ),
        integrations: IntegrationProviders::new(
            ProviderRegistration::new(
                ProviderId::borrowed("fixture.integration.command"),
                provider.clone(),
            ),
            ProviderRegistration::new(
                ProviderId::borrowed("fixture.integration.resource-reveal"),
                provider.clone(),
            ),
            ProviderRegistration::new(
                ProviderId::borrowed("fixture.integration.url-open"),
                provider.clone(),
            ),
            ProviderRegistration::new(
                ProviderId::borrowed("fixture.integration.desktop-appearance"),
                provider.clone(),
            ),
        ),
        storage: StorageProviders::new(
            ProviderRegistration::new(
                ProviderId::borrowed("fixture.storage.filesystem-health"),
                provider.clone(),
            ),
            ProviderRegistration::new(
                ProviderId::borrowed("fixture.storage.smart-observation"),
                provider.clone(),
            ),
            ProviderRegistration::new(
                ProviderId::borrowed("fixture.storage.smart-control"),
                provider.clone(),
            ),
            ProviderRegistration::new(
                ProviderId::borrowed("fixture.storage.directory-usage"),
                provider.clone(),
            ),
        ),
        sensors: SensorProviders::new(ProviderRegistration::new(
            ProviderId::borrowed("fixture.sensor"),
            provider.clone(),
        )),
        power: PowerProviders::new(ProviderRegistration::new(
            ProviderId::borrowed("fixture.power-supply"),
            provider.clone(),
        )),
    })
}
