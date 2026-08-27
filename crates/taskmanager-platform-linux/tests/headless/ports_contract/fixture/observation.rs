//! System observation providers: host / CPU / memory / storage / network / GPU
//! telemetry and hardware inventory.

use super::*;

impl HostTelemetryProvider for FakeProvider {
    fn refresh(&mut self, observed_at_ms: u64) -> Result<HostRuntimeObservation, ProviderFailure> {
        let sources = vec![fixture_source(
            "fixture.telemetry.host",
            1,
            self.observation_source_failure,
        )];
        Ok(self.observation_source_failure.map_or_else(
            || {
                HostRuntimeObservation::current(
                    HostRuntimeFacts::default(),
                    observed_at_ms,
                    sources.clone(),
                )
            },
            |failure| {
                HostRuntimeObservation::partial(
                    HostRuntimeFacts::default(),
                    observed_at_ms,
                    failure,
                    sources.clone(),
                )
            },
        ))
    }
}

impl CpuTelemetryProvider for FakeProvider {
    fn refresh(&mut self, observed_at_ms: u64) -> Result<CpuTelemetryObservation, ProviderFailure> {
        let sources = vec![fixture_source(
            "fixture.telemetry.cpu",
            1,
            self.observation_source_failure,
        )];
        Ok(self.observation_source_failure.map_or_else(
            || {
                CpuTelemetryObservation::current(
                    CpuMetrics::default(),
                    observed_at_ms,
                    sources.clone(),
                )
            },
            |failure| {
                CpuTelemetryObservation::partial(
                    CpuMetrics::default(),
                    observed_at_ms,
                    failure,
                    sources.clone(),
                )
            },
        ))
    }
}

impl MemoryTelemetryProvider for FakeProvider {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<MemoryTelemetryObservation, ProviderFailure> {
        let sources = vec![fixture_source(
            "fixture.telemetry.memory",
            1,
            self.observation_source_failure,
        )];
        Ok(self.observation_source_failure.map_or_else(
            || {
                MemoryTelemetryObservation::current(
                    MemoryMetrics::default(),
                    observed_at_ms,
                    sources.clone(),
                )
            },
            |failure| {
                MemoryTelemetryObservation::partial(
                    MemoryMetrics::default(),
                    observed_at_ms,
                    failure,
                    sources.clone(),
                )
            },
        ))
    }
}

impl StorageTelemetryProvider for FakeProvider {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<StorageTelemetryObservation, ProviderFailure> {
        thread::sleep(self.storage_observation_delay);
        let sources = vec![fixture_source(
            "fixture.telemetry.storage",
            0,
            self.observation_source_failure,
        )];
        Ok(self.observation_source_failure.map_or_else(
            || {
                StorageTelemetryObservation::current(
                    Vec::new(),
                    observed_at_ms,
                    sources.clone(),
                    Vec::new(),
                    Default::default(),
                )
            },
            |failure| {
                StorageTelemetryObservation::partial(
                    Vec::new(),
                    observed_at_ms,
                    failure,
                    sources.clone(),
                    Vec::new(),
                    Default::default(),
                )
            },
        ))
    }
}

impl NetworkTelemetryProvider for FakeProvider {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<NetworkTelemetryObservation, ProviderFailure> {
        let sources = vec![fixture_source(
            "fixture.telemetry.network",
            0,
            self.observation_source_failure,
        )];
        Ok(self.observation_source_failure.map_or_else(
            || {
                NetworkTelemetryObservation::current(
                    Vec::new(),
                    observed_at_ms,
                    sources.clone(),
                    Vec::new(),
                    Default::default(),
                )
            },
            |failure| {
                NetworkTelemetryObservation::partial(
                    Vec::new(),
                    observed_at_ms,
                    failure,
                    sources.clone(),
                    Vec::new(),
                    Default::default(),
                )
            },
        ))
    }
}

impl GpuTelemetryProvider for FakeProvider {
    fn refresh(&mut self, observed_at_ms: u64) -> Result<GpuTelemetryObservation, ProviderFailure> {
        thread::sleep(self.gpu_observation_delay);
        let sources = vec![fixture_source(
            "fixture.telemetry.gpu",
            0,
            self.observation_source_failure,
        )];
        Ok(self.observation_source_failure.map_or_else(
            || {
                GpuTelemetryObservation::current(
                    Vec::new(),
                    observed_at_ms,
                    sources.clone(),
                    Vec::new(),
                    Default::default(),
                )
            },
            |failure| {
                GpuTelemetryObservation::partial(
                    Vec::new(),
                    observed_at_ms,
                    failure,
                    sources.clone(),
                    Vec::new(),
                    Default::default(),
                )
            },
        ))
    }
}

impl GpuEngineRowsProvider for FakeProvider {
    fn read_engine_rows(
        &mut self,
        device_id: &DeviceId,
    ) -> Result<GpuEngineRowsSnapshot, ProviderFailure> {
        // One bounded immediate call: the contract fixture only needs the
        // engine-rows lane wired so the capability is published; it does not
        // exercise real PMU semantics.
        Ok(GpuEngineRowsSnapshot::success(
            device_id.clone(),
            vec![GpuEngineMetric {
                name: "fixture engine".to_owned(),
                kind: taskmanager_core::GpuEngineKind::Unknown,
                utilization_pct: 50.0,
            }],
        ))
    }
}

impl NpuInventoryProvider for FakeProvider {
    fn read_inventory(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<NpuInventorySnapshot, ProviderFailure> {
        // One bounded immediate call: the contract fixture only needs the
        // inventory lane wired so the capability is published; the empty list
        // is the honest no-NPU answer.
        Ok(NpuInventorySnapshot::discovered(Vec::new(), observed_at_ms))
    }
}

impl HardwareInventoryProvider for FakeProvider {
    fn refresh(&mut self) -> Result<CompositeSourceSnapshot<HardwareInfo>, ProviderFailure> {
        Ok(CompositeSourceSnapshot::new(
            HardwareInfo::default(),
            vec![fixture_source(
                "fixture.hardware",
                1,
                self.observation_source_failure,
            )],
        ))
    }
}

impl ContainerRollupProvider for FakeProvider {
    fn refresh(&mut self, _now_ms: u64) -> Result<ContainerRollup, ProviderFailure> {
        Ok(ContainerRollup::empty_healthy(0))
    }
}
