//! Typed cross-crate metric fixtures.
//!
//! These doc-hidden builders expose canonical observation groups and named
//! domain assembly only. They deliberately do not accept schema-v1 field
//! names or hydrate legacy sentinel values.

use std::{marker::PhantomData, sync::Arc};

use super::{
    DiskMetrics, DiskPartition, DiskPartitionScalarObservations, DiskScalarObservations,
    NetworkAdapterType, NetworkMetrics, NetworkScalarObservations, NetworkWirelessObservations,
    OptionalObservation, ScalarObservation, SmartAvailability,
};
use crate::{GroupBaseOpen, NamedOverrides};
use taskmanager_core::core::device_state::DeviceState;
use taskmanager_core::core::identity::{DeviceGeneration, ProviderId};
use taskmanager_core::core::smart::SmartProviderFailureKind;
use taskmanager_core::core::storage::{StorageConnection, StorageIdentityStability};

const FIXTURE_OBSERVED_AT: u64 = 1;

/// Canonical disk-row builder for cross-crate behavior fixtures.
#[doc(hidden)]
#[derive(Debug)]
pub struct DiskMetricsFixtureBuilder<ScalarStage = GroupBaseOpen> {
    item: DiskMetrics,
    scalars: DiskScalarObservations,
    connection: StorageConnection,
    media_removable: Option<bool>,
    hotplug_capable: Option<bool>,
    scalar_stage: PhantomData<ScalarStage>,
}

impl Default for DiskMetricsFixtureBuilder {
    fn default() -> Self {
        Self {
            item: DiskMetrics::default(),
            scalars: DiskScalarObservations::default(),
            connection: StorageConnection::default(),
            media_removable: None,
            hotplug_capable: None,
            scalar_stage: PhantomData,
        }
    }
}

impl DiskMetricsFixtureBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn from_item(item: DiskMetrics) -> Self {
        let scalars = *item.scalar_observations();
        let connection = item.connection();
        let media_removable = item.media_removable();
        let hotplug_capable = item.hotplug_capable();
        Self {
            item,
            scalars,
            connection,
            media_removable,
            hotplug_capable,
            scalar_stage: PhantomData,
        }
    }
}

impl<ScalarStage> DiskMetricsFixtureBuilder<ScalarStage> {
    fn retag<NextScalar>(self) -> DiskMetricsFixtureBuilder<NextScalar> {
        DiskMetricsFixtureBuilder {
            item: self.item,
            scalars: self.scalars,
            connection: self.connection,
            media_removable: self.media_removable,
            hotplug_capable: self.hotplug_capable,
            scalar_stage: PhantomData,
        }
    }

    #[must_use]
    pub fn device_id(mut self, value: String) -> Self {
        self.item.device_id = value;
        self
    }
    #[must_use]
    pub fn device_generation(mut self, value: DeviceGeneration) -> Self {
        self.item.device_generation = value;
        self
    }
    #[must_use]
    pub fn device_state(mut self, value: DeviceState) -> Self {
        self.item.device_state = value;
        self
    }
    #[must_use]
    pub fn name(mut self, value: String) -> Self {
        self.item.name = value;
        self
    }
    #[must_use]
    pub fn disk_type(mut self, value: String) -> Self {
        self.item.disk_type = value;
        self
    }
    #[must_use]
    pub fn connection(mut self, value: StorageConnection) -> Self {
        self.connection = value;
        self
    }
    #[must_use]
    pub fn identity_stability(mut self, value: StorageIdentityStability) -> Self {
        self.item.identity_stability = value;
        self
    }
    #[must_use]
    pub fn model(mut self, value: String) -> Self {
        self.item.model = value;
        self
    }
    #[must_use]
    pub fn serial(mut self, value: Option<String>) -> Self {
        self.item.serial = value;
        self
    }
    #[must_use]
    pub fn revision(mut self, value: Option<String>) -> Self {
        self.item.revision = value;
        self
    }
    #[must_use]
    pub fn mount_point(mut self, value: String) -> Self {
        self.item.mount_point = value;
        self
    }
    #[must_use]
    pub fn fs_type(mut self, value: String) -> Self {
        self.item.fs_type = value;
        self
    }
    #[must_use]
    pub fn partitions(mut self, value: Vec<DiskPartition>) -> Self {
        self.item.partitions = value;
        self
    }
    #[must_use]
    pub fn media_removable(mut self, value: Option<bool>) -> Self {
        self.media_removable = value;
        self
    }
    #[must_use]
    pub fn hotplug_capable(mut self, value: Option<bool>) -> Self {
        self.hotplug_capable = value;
        self
    }
    #[must_use]
    pub fn smart_availability(mut self, value: SmartAvailability) -> Self {
        self.item.smart_availability = value;
        self
    }
    #[must_use]
    pub fn smart_state(mut self, value: DeviceState) -> Self {
        self.item.smart_state = value;
        self
    }
    #[must_use]
    pub fn smart_provider(mut self, value: Option<ProviderId>) -> Self {
        self.item.smart_provider = value;
        self
    }
    #[must_use]
    pub fn smart_failure(mut self, value: Option<SmartProviderFailureKind>) -> Self {
        self.item.smart_failure = value;
        self
    }
    #[must_use]
    pub fn smart_temperature_c(mut self, value: Option<f32>) -> Self {
        self.item.smart_temperature_c = value;
        self
    }
    #[must_use]
    pub fn smart_critical_warning(mut self, value: Option<bool>) -> Self {
        self.item.smart_critical_warning = value;
        self
    }
    #[must_use]
    pub fn smart_temp_critical_c(mut self, value: Option<f32>) -> Self {
        self.item.smart_temp_critical_c = value;
        self
    }
    #[must_use]
    pub fn smart_percent_used(mut self, value: Option<f32>) -> Self {
        self.item.smart_percent_used = value;
        self
    }
    #[must_use]
    pub fn smart_power_on_hours(mut self, value: Option<u64>) -> Self {
        self.item.smart_power_on_hours = value;
        self
    }

    #[must_use]
    pub fn build(mut self) -> DiskMetrics {
        self.item.apply_connection(self.connection);
        self.item
            .apply_attachment_capabilities(self.media_removable, self.hotplug_capable);
        self.item.apply_scalar_observations(self.scalars);
        self.item
    }
}

impl DiskMetricsFixtureBuilder<GroupBaseOpen> {
    /// Install the optional scalar base and enter the named-override stage.
    #[must_use]
    pub fn scalar_observations(
        self,
        value: DiskScalarObservations,
    ) -> DiskMetricsFixtureBuilder<NamedOverrides> {
        let mut next = self.retag();
        next.scalars = value;
        next
    }

    #[must_use]
    pub fn current_capacity_bytes(self, value: u64) -> DiskMetricsFixtureBuilder<NamedOverrides> {
        let next: DiskMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.current_capacity_bytes(value)
    }

    #[must_use]
    pub fn current_available_bytes(self, value: u64) -> DiskMetricsFixtureBuilder<NamedOverrides> {
        let next: DiskMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.current_available_bytes(value)
    }

    #[must_use]
    pub fn current_read_bytes_per_sec(
        self,
        value: u64,
    ) -> DiskMetricsFixtureBuilder<NamedOverrides> {
        let next: DiskMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.current_read_bytes_per_sec(value)
    }

    #[must_use]
    pub fn current_write_bytes_per_sec(
        self,
        value: u64,
    ) -> DiskMetricsFixtureBuilder<NamedOverrides> {
        let next: DiskMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.current_write_bytes_per_sec(value)
    }

    #[must_use]
    pub fn current_iops(self, value: u64) -> DiskMetricsFixtureBuilder<NamedOverrides> {
        let next: DiskMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.current_iops(value)
    }

    #[must_use]
    pub fn current_active_time_pct(self, value: f32) -> DiskMetricsFixtureBuilder<NamedOverrides> {
        let next: DiskMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.current_active_time_pct(value)
    }

    #[must_use]
    pub fn current_response_time_ms(self, value: f32) -> DiskMetricsFixtureBuilder<NamedOverrides> {
        let next: DiskMetricsFixtureBuilder<NamedOverrides> = self.retag();
        next.current_response_time_ms(value)
    }
}

impl DiskMetricsFixtureBuilder<NamedOverrides> {
    #[must_use]
    pub fn current_capacity_bytes(mut self, value: u64) -> Self {
        self.scalars.capacity_bytes = ScalarObservation::available(value, FIXTURE_OBSERVED_AT);
        self
    }

    #[must_use]
    pub fn current_available_bytes(mut self, value: u64) -> Self {
        self.scalars.available_bytes = ScalarObservation::available(value, FIXTURE_OBSERVED_AT);
        self
    }

    #[must_use]
    pub fn current_read_bytes_per_sec(mut self, value: u64) -> Self {
        self.scalars.read_bytes_per_sec = ScalarObservation::available(value, FIXTURE_OBSERVED_AT);
        self
    }

    #[must_use]
    pub fn current_write_bytes_per_sec(mut self, value: u64) -> Self {
        self.scalars.write_bytes_per_sec = ScalarObservation::available(value, FIXTURE_OBSERVED_AT);
        self
    }

    #[must_use]
    pub fn current_iops(mut self, value: u64) -> Self {
        self.scalars.iops = ScalarObservation::available(value, FIXTURE_OBSERVED_AT);
        self
    }

    #[must_use]
    pub fn current_active_time_pct(mut self, value: f32) -> Self {
        self.scalars.active_time_pct = ScalarObservation::available(value, FIXTURE_OBSERVED_AT);
        self
    }

    #[must_use]
    pub fn current_response_time_ms(mut self, value: f32) -> Self {
        self.scalars.response_time_ms = ScalarObservation::available(value, FIXTURE_OBSERVED_AT);
        self
    }
}

/// Canonical partition-row builder for cross-crate behavior fixtures.
#[doc(hidden)]
#[derive(Debug)]
pub struct DiskPartitionFixtureBuilder<ScalarStage = GroupBaseOpen> {
    item: DiskPartition,
    scalars: DiskPartitionScalarObservations,
    scalar_stage: PhantomData<ScalarStage>,
}

impl Default for DiskPartitionFixtureBuilder {
    fn default() -> Self {
        Self {
            item: DiskPartition::default(),
            scalars: DiskPartitionScalarObservations::default(),
            scalar_stage: PhantomData,
        }
    }
}

impl DiskPartitionFixtureBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    #[must_use]
    pub fn from_item(item: DiskPartition) -> Self {
        let scalars = *item.scalar_observations();
        Self {
            item,
            scalars,
            scalar_stage: PhantomData,
        }
    }
}

impl<ScalarStage> DiskPartitionFixtureBuilder<ScalarStage> {
    fn retag<NextScalar>(self) -> DiskPartitionFixtureBuilder<NextScalar> {
        DiskPartitionFixtureBuilder {
            item: self.item,
            scalars: self.scalars,
            scalar_stage: PhantomData,
        }
    }
    #[must_use]
    pub fn device_id(mut self, value: String) -> Self {
        self.item.device_id = value;
        self
    }
    #[must_use]
    pub fn parent_device_id(mut self, value: String) -> Self {
        self.item.parent_device_id = value;
        self
    }
    #[must_use]
    pub fn device_generation(mut self, value: DeviceGeneration) -> Self {
        self.item.device_generation = value;
        self
    }
    #[must_use]
    pub fn device_state(mut self, value: DeviceState) -> Self {
        self.item.device_state = value;
        self
    }
    #[must_use]
    pub fn name(mut self, value: String) -> Self {
        self.item.name = value;
        self
    }
    #[must_use]
    pub fn mount_point(mut self, value: String) -> Self {
        self.item.mount_point = value;
        self
    }
    #[must_use]
    pub fn fs_type(mut self, value: String) -> Self {
        self.item.fs_type = value;
        self
    }
    #[must_use]
    pub fn build(mut self) -> DiskPartition {
        self.item.apply_scalar_observations(self.scalars);
        self.item
    }
}

impl DiskPartitionFixtureBuilder<GroupBaseOpen> {
    /// Install the optional scalar base and enter the named-override stage.
    #[must_use]
    pub fn scalar_observations(
        self,
        value: DiskPartitionScalarObservations,
    ) -> DiskPartitionFixtureBuilder<NamedOverrides> {
        let mut next = self.retag();
        next.scalars = value;
        next
    }

    #[must_use]
    pub fn current_capacity_bytes(self, value: u64) -> DiskPartitionFixtureBuilder<NamedOverrides> {
        let next: DiskPartitionFixtureBuilder<NamedOverrides> = self.retag();
        next.current_capacity_bytes(value)
    }

    #[must_use]
    pub fn current_used_bytes(self, value: u64) -> DiskPartitionFixtureBuilder<NamedOverrides> {
        let next: DiskPartitionFixtureBuilder<NamedOverrides> = self.retag();
        next.current_used_bytes(value)
    }

    #[must_use]
    pub fn current_free_bytes(self, value: u64) -> DiskPartitionFixtureBuilder<NamedOverrides> {
        let next: DiskPartitionFixtureBuilder<NamedOverrides> = self.retag();
        next.current_free_bytes(value)
    }
}

impl DiskPartitionFixtureBuilder<NamedOverrides> {
    #[must_use]
    pub fn current_capacity_bytes(mut self, value: u64) -> Self {
        self.scalars.capacity_bytes = ScalarObservation::available(value, FIXTURE_OBSERVED_AT);
        self
    }

    #[must_use]
    pub fn current_used_bytes(mut self, value: u64) -> Self {
        self.scalars.used_bytes = ScalarObservation::available(value, FIXTURE_OBSERVED_AT);
        self
    }

    #[must_use]
    pub fn current_free_bytes(mut self, value: u64) -> Self {
        self.scalars.free_bytes = ScalarObservation::available(value, FIXTURE_OBSERVED_AT);
        self
    }
}

/// Canonical network-row builder for cross-crate behavior fixtures.
#[doc(hidden)]
#[derive(Debug, Default)]
pub struct NetworkMetricsFixtureBuilder {
    item: NetworkMetrics,
    adapter_type: NetworkAdapterType,
    scalars: NetworkScalarObservations,
    wireless: NetworkWirelessObservations,
}

impl NetworkMetricsFixtureBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    #[must_use]
    pub fn from_item(item: NetworkMetrics) -> Self {
        let adapter_type = item.adapter_type();
        let scalars = *item.scalar_observations();
        let wireless = item.wireless_observations().clone();
        Self {
            item,
            adapter_type,
            scalars,
            wireless,
        }
    }
    #[must_use]
    pub fn device_id(mut self, value: Arc<str>) -> Self {
        self.item.device_id = value;
        self
    }
    #[must_use]
    pub fn device_generation(mut self, value: DeviceGeneration) -> Self {
        self.item.device_generation = value;
        self
    }
    #[must_use]
    pub fn device_state(mut self, value: DeviceState) -> Self {
        self.item.device_state = value;
        self
    }
    #[must_use]
    pub fn interface_name(mut self, value: Arc<str>) -> Self {
        self.item.interface_name = value;
        self
    }
    #[must_use]
    pub fn ipv4_addr(mut self, value: Option<Arc<str>>) -> Self {
        self.item.ipv4_addr = value;
        self
    }
    #[must_use]
    pub fn ipv6_addr(mut self, value: Option<Arc<str>>) -> Self {
        self.item.ipv6_addr = value;
        self
    }
    #[must_use]
    pub fn mac_addr(mut self, value: Option<Arc<str>>) -> Self {
        self.item.mac_addr = value;
        self
    }
    #[must_use]
    pub fn driver(mut self, value: Option<Arc<str>>) -> Self {
        self.item.driver = value;
        self
    }
    #[must_use]
    pub fn adapter(mut self, value: Option<Arc<str>>) -> Self {
        self.item.adapter = value;
        self
    }
    #[must_use]
    pub fn adapter_type(mut self, value: NetworkAdapterType) -> Self {
        self.adapter_type = value;
        self
    }
    #[must_use]
    pub fn scalar_observations(mut self, value: NetworkScalarObservations) -> Self {
        self.scalars = value;
        self
    }
    #[must_use]
    pub fn wireless_observations(mut self, value: NetworkWirelessObservations) -> Self {
        self.wireless = value;
        self
    }
    #[must_use]
    pub fn current_total_rx_bytes(mut self, value: u64) -> Self {
        self.scalars.total_rx_bytes = ScalarObservation::available(value, FIXTURE_OBSERVED_AT);
        self
    }
    #[must_use]
    pub fn current_total_tx_bytes(mut self, value: u64) -> Self {
        self.scalars.total_tx_bytes = ScalarObservation::available(value, FIXTURE_OBSERVED_AT);
        self
    }
    #[must_use]
    pub fn current_rx_bytes_per_sec(mut self, value: u64) -> Self {
        self.scalars.rx_bytes_per_sec = ScalarObservation::available(value, FIXTURE_OBSERVED_AT);
        self
    }
    #[must_use]
    pub fn current_tx_bytes_per_sec(mut self, value: u64) -> Self {
        self.scalars.tx_bytes_per_sec = ScalarObservation::available(value, FIXTURE_OBSERVED_AT);
        self
    }
    #[must_use]
    pub fn current_utilization_pct(mut self, value: f32) -> Self {
        self.scalars.utilization_pct = ScalarObservation::available(value, FIXTURE_OBSERVED_AT);
        self
    }
    #[must_use]
    pub fn link_speed_observation(mut self, value: ScalarObservation<u64>) -> Self {
        self.scalars.link_speed_mbps = value;
        self
    }
    #[must_use]
    pub fn link_up_observation(mut self, value: ScalarObservation<bool>) -> Self {
        self.scalars.link_up = value;
        self
    }
    #[must_use]
    pub fn ssid_observation(mut self, value: OptionalObservation<Arc<str>>) -> Self {
        if value.current_value().is_some() && self.wireless.association.current_value().is_none() {
            self.wireless.association = OptionalObservation::present(true, FIXTURE_OBSERVED_AT);
        }
        self.wireless.ssid = value;
        self
    }
    #[must_use]
    pub fn signal_observation(mut self, value: OptionalObservation<i32>) -> Self {
        self.wireless.signal_dbm = value;
        self
    }
    #[must_use]
    pub fn build(mut self) -> NetworkMetrics {
        self.item
            .apply_observations(self.adapter_type, self.scalars, self.wireless);
        self.item
    }
}
