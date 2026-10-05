//! Concrete device preferences over the shared configuration authority.

use super::{IcedApp, LocalSurfaceKind, PerfDevice};
use taskmanager_application::i18n::t;
use taskmanager_core::core::config::sidebar::{
    move_sidebar_order, ordered_indices, set_sidebar_override, visible_with_override,
};
use taskmanager_core::core::metrics::NetworkAdapterType;
use taskmanager_core::core::sensors::SensorQuantity;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SidebarEntry {
    pub(crate) key: String,
    pub(crate) device: PerfDevice,
    pub(crate) label: String,
    pub(crate) visible: bool,
}

impl IcedApp {
    pub(crate) fn sidebar_entries(&self) -> Vec<SidebarEntry> {
        let config = self.configuration.draft();
        let mut entries = Vec::new();
        let mut add = |key: String, device, label, category| {
            entries.push(SidebarEntry {
                visible: visible_with_override(&key, category, &config.sidebar_device_overrides),
                key,
                device,
                label,
            });
        };
        add(
            "cpu".into(),
            PerfDevice::Cpu,
            t("common.cpu").into(),
            config.show_cpu,
        );
        add(
            "memory".into(),
            PerfDevice::Memory,
            t("common.memory").into(),
            config.show_memory,
        );
        let projection = self.shell.projection();
        if let Some(snapshot) = &projection.snapshot {
            for (index, disk) in snapshot.disks.iter().enumerate() {
                add(
                    format!("disk:{}", disk.device_id),
                    PerfDevice::Disk(index),
                    disk.name.clone(),
                    config.show_disks,
                );
            }
            for (index, network) in snapshot.networks.iter().enumerate() {
                let category = config.show_network
                    && match network.adapter_type() {
                        NetworkAdapterType::Ethernet => config.show_network_wired,
                        NetworkAdapterType::WiFi => config.show_network_wireless,
                        NetworkAdapterType::Vpn => config.show_network_vpn,
                        NetworkAdapterType::Virtual => config.show_network_virtual,
                        NetworkAdapterType::Unknown
                        | NetworkAdapterType::Loopback
                        | NetworkAdapterType::Other => config.show_network_other,
                    };
                add(
                    format!("network:{}", network.device_id),
                    PerfDevice::Network(index),
                    network.interface_name.to_string(),
                    category,
                );
            }
            for (index, gpu) in snapshot.gpu.iter().enumerate() {
                add(
                    format!("gpu:{}", gpu.device_id),
                    PerfDevice::Gpu(index),
                    gpu.marketing_name.as_ref().unwrap_or(&gpu.brand).clone(),
                    config.show_gpus,
                );
            }
        }
        if let Some(npu) = &projection.npu_inventory
            && npu.is_success()
        {
            for (index, device) in npu.devices.iter().enumerate() {
                add(
                    format!("npu:{}", device.device_id),
                    PerfDevice::Npu(index),
                    format!("NPU {}", device.device_id),
                    true,
                );
            }
        }
        if let Some(power) = &projection.power_supplies {
            for (index, battery) in power.batteries.iter().enumerate() {
                add(
                    format!("battery:{}", battery.id),
                    PerfDevice::Battery(index),
                    battery.id.clone(),
                    true,
                );
            }
        }
        if let Some(sensors) = &projection.sensors {
            for (index, fan) in sensors
                .readings
                .iter()
                .filter(|reading| reading.quantity() == &SensorQuantity::FanSpeed)
                .enumerate()
            {
                add(
                    format!("fan:{}", fan.id()),
                    PerfDevice::Fan(index),
                    fan.id().to_owned(),
                    true,
                );
            }
        }
        let keys: Vec<_> = entries.iter().map(|entry| entry.key.clone()).collect();
        ordered_indices(&keys, &config.sidebar_order)
            .into_iter()
            .map(|index| entries[index].clone())
            .collect()
    }

    pub(super) fn set_sidebar_device_visibility(&mut self, key: &str, visible: bool) {
        if self.local_surface_kind() != Some(LocalSurfaceKind::SidebarEditor)
            || !self.sidebar_entries().iter().any(|entry| entry.key == key)
        {
            return;
        }
        let mut config = self.config_draft();
        set_sidebar_override(&mut config.sidebar_device_overrides, key, visible);
        self.commit_config_draft(config);
    }

    pub(super) fn move_sidebar_device(&mut self, key: &str, delta: isize) {
        if self.local_surface_kind() != Some(LocalSurfaceKind::SidebarEditor) {
            return;
        }
        let live: Vec<_> = self
            .sidebar_entries()
            .into_iter()
            .map(|entry| entry.key)
            .collect();
        let mut config = self.config_draft();
        if let Some(order) = move_sidebar_order(&live, &config.sidebar_order, key, delta) {
            config.sidebar_order = order;
            self.commit_config_draft(config);
        }
    }
}

#[cfg(test)]
#[path = "../../tests/gui/app/sidebar_tests.rs"]
mod tests;
