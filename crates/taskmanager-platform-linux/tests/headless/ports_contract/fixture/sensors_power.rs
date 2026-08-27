//! Sensor and power-supply providers.

use super::*;

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
        Ok(DeviceSourceSnapshot::from_source_status(
            SensorCenterSnapshot::default(),
            Vec::new(),
            SourceStatus {
                provider: ProviderId::borrowed("fixture.sensor.discovery"),
                outcome: SourceOutcome::Empty,
                item_count: 0,
            },
            enrichments,
        ))
    }
}

impl PowerSupplyProvider for FakeProvider {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<DeviceSourceSnapshot<PowerSupplySnapshot>, ProviderFailure> {
        Ok(DeviceSourceSnapshot::from_source_status(
            PowerSupplySnapshot {
                state: DeviceState::healthy(observed_at_ms),
                timestamp_ms: observed_at_ms,
                batteries: Vec::new(),
                device_lifecycles: Default::default(),
            },
            Vec::new(),
            SourceStatus {
                provider: ProviderId::borrowed("fixture.power.discovery"),
                outcome: SourceOutcome::Empty,
                item_count: 0,
            },
            Vec::new(),
        ))
    }
}
