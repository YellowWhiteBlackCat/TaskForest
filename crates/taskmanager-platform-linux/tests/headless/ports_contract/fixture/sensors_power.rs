//! Sensor and power-supply providers.

use super::*;
use taskmanager_platform_contract::DeviceDiscovery;

impl SensorProvider for FakeProvider {
    fn refresh(
        &mut self,
        _observed_at_ms: u64,
    ) -> Result<DeviceSourceSnapshot<SensorCenterSnapshot>, ProviderFailure> {
        let enrichments = self
            .sensor_enrichment_error
            .map(|failure| SourceStatus {
                provider: ProviderId::borrowed("fixture.sensor.enrichment"),
                outcome: SourceOutcome::Unavailable(failure),
                item_count: 0,
            })
            .into_iter()
            .collect();
        Ok(DeviceSourceSnapshot::from_discovery(
            SensorCenterSnapshot::default(),
            ProviderId::borrowed("fixture.sensor.discovery"),
            DeviceDiscovery::Empty,
            enrichments,
        ))
    }
}

impl PowerSupplyProvider for FakeProvider {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<DeviceSourceSnapshot<PowerSupplySnapshot>, ProviderFailure> {
        Ok(DeviceSourceSnapshot::from_discovery(
            PowerSupplySnapshot {
                state: DeviceState::healthy(observed_at_ms),
                timestamp_ms: observed_at_ms,
                batteries: Vec::new(),
                device_lifecycles: Default::default(),
            },
            ProviderId::borrowed("fixture.power.discovery"),
            DeviceDiscovery::Empty,
            Vec::new(),
        ))
    }
}
