//! The device-category visibility toggles: one vocabulary, one preference
//! projection, one config fold.
//!
//! The Performance sidebar shows a CPU / Memory / Disks / Network / GPUs
//! column set, and the Network column expands into the five Mission Center
//! sub-categories. That set of ten switches is a PRODUCT fact, and it lived
//! in three places at once: the ten `Config::show_*` fields, GPUI's private
//! `DevicePreference` enum with its ten-way visibility match, and Iced's
//! `DeviceKind` enum with the same ten-way match. Two frontends spelled the
//! same ten categories twice, so the set could drift from the config schema
//! and from the other shape.
//!
//! This module is the one owner: [`DeviceVisibilityKey`] names the ten
//! categories and their order, [`DeviceVisibilityPreferences`] holds the ten
//! switches, and the `Config` fold reads and writes them. A renderer binds
//! these and spends the order; it never restates the set.

use super::Config;
use crate::core::metrics::NetworkAdapterType;

/// One device category whose sidebar visibility the user can toggle.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum DeviceVisibilityKey {
    /// The CPU column.
    Cpu,
    /// The memory column.
    Memory,
    /// The disk columns.
    Disks,
    /// The master network toggle.
    Network,
    /// Wired network interfaces.
    NetworkWired,
    /// Wireless network interfaces.
    NetworkWireless,
    /// VPN network interfaces.
    NetworkVpn,
    /// Virtual network interfaces.
    NetworkVirtual,
    /// Every other network interface.
    NetworkOther,
    /// The GPU columns.
    Gpus,
}

impl DeviceVisibilityKey {
    /// Every category, in the Settings row order the product presents.
    pub const ALL: [Self; 10] = [
        Self::Cpu,
        Self::Memory,
        Self::Disks,
        Self::Network,
        Self::NetworkWired,
        Self::NetworkWireless,
        Self::NetworkVpn,
        Self::NetworkVirtual,
        Self::NetworkOther,
        Self::Gpus,
    ];

    /// Locale catalog key naming this category's toggle.
    #[must_use]
    pub const fn i18n_key(self) -> &'static str {
        match self {
            Self::Cpu => "settings.show_cpu",
            Self::Memory => "settings.show_memory",
            Self::Disks => "settings.show_disks",
            Self::Network => "settings.show_network",
            Self::NetworkWired => "settings.show_network_wired",
            Self::NetworkWireless => "settings.show_network_wireless",
            Self::NetworkVpn => "settings.show_network_vpn",
            Self::NetworkVirtual => "settings.show_network_virtual",
            Self::NetworkOther => "settings.show_network_other",
            Self::Gpus => "settings.show_gpus",
        }
    }

    /// The stable, non-localized widget id of this category's toggle.
    ///
    /// Both graphical shapes key their Settings switch (and their focus
    /// target) by this id, so a widget identity is a product fact too: a
    /// frontend that spelled its own ids could not be compared with the
    /// other shape's.
    #[must_use]
    pub const fn stable_id(self) -> &'static str {
        match self {
            Self::Cpu => "device-cpu",
            Self::Memory => "device-memory",
            Self::Disks => "device-disks",
            Self::Network => "device-network",
            Self::NetworkWired => "network-wired",
            Self::NetworkWireless => "network-wireless",
            Self::NetworkVpn => "network-vpn",
            Self::NetworkVirtual => "network-virtual",
            Self::NetworkOther => "network-other",
            Self::Gpus => "device-gpus",
        }
    }

    /// The network sub-categories, in Settings row order.
    ///
    /// The master network toggle is NOT a sub-category: it governs the
    /// whole column, while these five classify the interfaces inside it.
    pub const NETWORK_SUBCATEGORIES: [Self; 5] = [
        Self::NetworkWired,
        Self::NetworkWireless,
        Self::NetworkVpn,
        Self::NetworkVirtual,
        Self::NetworkOther,
    ];

    /// Whether this category is one of the network sub-categories.
    #[must_use]
    pub const fn is_network_subcategory(self) -> bool {
        matches!(
            self,
            Self::NetworkWired
                | Self::NetworkWireless
                | Self::NetworkVpn
                | Self::NetworkVirtual
                | Self::NetworkOther
        )
    }

    /// The network sub-category switch that classifies one adapter type.
    ///
    /// This is the single adapter-type → sub-category fold: every frontend
    /// that filters its device rail by interface class spends it instead of
    /// writing its own `Ethernet → wired / WiFi → wireless / …` match.
    #[must_use]
    pub const fn for_network_adapter(adapter: NetworkAdapterType) -> Self {
        match adapter {
            NetworkAdapterType::Ethernet => Self::NetworkWired,
            NetworkAdapterType::WiFi => Self::NetworkWireless,
            NetworkAdapterType::Vpn => Self::NetworkVpn,
            NetworkAdapterType::Virtual => Self::NetworkVirtual,
            NetworkAdapterType::Unknown
            | NetworkAdapterType::Loopback
            | NetworkAdapterType::Other => Self::NetworkOther,
        }
    }
}

/// The ten device-category visibility switches.
///
/// This is the render-neutral projection of the `Config::show_*` fields: a
/// shape reads it, flips one switch, and writes it back. It owns the set of
/// slots, so no renderer keeps a second ten-way match.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceVisibilityPreferences {
    /// The CPU column is visible.
    pub cpu: bool,
    /// The memory column is visible.
    pub memory: bool,
    /// The disk columns are visible.
    pub disks: bool,
    /// The network column is visible.
    pub network: bool,
    /// Wired network interfaces are visible.
    pub network_wired: bool,
    /// Wireless network interfaces are visible.
    pub network_wireless: bool,
    /// VPN network interfaces are visible.
    pub network_vpn: bool,
    /// Virtual network interfaces are visible.
    pub network_virtual: bool,
    /// Every other network interface is visible.
    pub network_other: bool,
    /// The GPU columns are visible.
    pub gpus: bool,
}

impl Default for DeviceVisibilityPreferences {
    fn default() -> Self {
        Self {
            cpu: true,
            memory: true,
            disks: true,
            network: true,
            network_wired: true,
            network_wireless: true,
            network_vpn: true,
            network_virtual: true,
            network_other: true,
            gpus: true,
        }
    }
}

impl DeviceVisibilityPreferences {
    /// Read the ten switches out of the persisted config.
    #[must_use]
    pub const fn from_config(config: &Config) -> Self {
        Self {
            cpu: config.show_cpu,
            memory: config.show_memory,
            disks: config.show_disks,
            network: config.show_network,
            network_wired: config.show_network_wired,
            network_wireless: config.show_network_wireless,
            network_vpn: config.show_network_vpn,
            network_virtual: config.show_network_virtual,
            network_other: config.show_network_other,
            gpus: config.show_gpus,
        }
    }

    /// Whether one category is visible.
    #[must_use]
    pub const fn visible(self, key: DeviceVisibilityKey) -> bool {
        match key {
            DeviceVisibilityKey::Cpu => self.cpu,
            DeviceVisibilityKey::Memory => self.memory,
            DeviceVisibilityKey::Disks => self.disks,
            DeviceVisibilityKey::Network => self.network,
            DeviceVisibilityKey::NetworkWired => self.network_wired,
            DeviceVisibilityKey::NetworkWireless => self.network_wireless,
            DeviceVisibilityKey::NetworkVpn => self.network_vpn,
            DeviceVisibilityKey::NetworkVirtual => self.network_virtual,
            DeviceVisibilityKey::NetworkOther => self.network_other,
            DeviceVisibilityKey::Gpus => self.gpus,
        }
    }

    /// Whether one network interface's class is visible.
    ///
    /// The master network toggle governs the whole column; this fold adds
    /// the sub-category switch for the interface's adapter type.
    #[must_use]
    pub const fn network_visible(self, adapter: NetworkAdapterType) -> bool {
        self.visible(DeviceVisibilityKey::Network)
            && self.visible(DeviceVisibilityKey::for_network_adapter(adapter))
    }

    /// Write the ten switches back into the persisted config.
    pub const fn apply_to(self, config: &mut Config) {
        config.show_cpu = self.cpu;
        config.show_memory = self.memory;
        config.show_disks = self.disks;
        config.show_network = self.network;
        config.show_network_wired = self.network_wired;
        config.show_network_wireless = self.network_wireless;
        config.show_network_vpn = self.network_vpn;
        config.show_network_virtual = self.network_virtual;
        config.show_network_other = self.network_other;
        config.show_gpus = self.gpus;
    }

    /// The ten switches as a Settings-form array in [`DeviceVisibilityKey::ALL`]
    /// order.
    ///
    /// A form that owns an indexed `[bool; 10]` (a terminal widget's binding)
    /// spends this fold instead of spelling the ten config fields again.
    #[must_use]
    pub const fn to_array(self) -> [bool; 10] {
        let mut values = [false; 10];
        let mut index = 0;
        while index < DeviceVisibilityKey::ALL.len() {
            values[index] = self.visible(DeviceVisibilityKey::ALL[index]);
            index += 1;
        }
        values
    }

    /// Rebuild the ten switches from a Settings-form array in
    /// [`DeviceVisibilityKey::ALL`] order.
    #[must_use]
    pub const fn from_array(values: [bool; 10]) -> Self {
        Self {
            cpu: values[0],
            memory: values[1],
            disks: values[2],
            network: values[3],
            network_wired: values[4],
            network_wireless: values[5],
            network_vpn: values[6],
            network_virtual: values[7],
            network_other: values[8],
            gpus: values[9],
        }
    }

    /// Flip one category's switch. Returns whether the value changed.
    pub const fn set_visible(&mut self, key: DeviceVisibilityKey, visible: bool) -> bool {
        let target = match key {
            DeviceVisibilityKey::Cpu => &mut self.cpu,
            DeviceVisibilityKey::Memory => &mut self.memory,
            DeviceVisibilityKey::Disks => &mut self.disks,
            DeviceVisibilityKey::Network => &mut self.network,
            DeviceVisibilityKey::NetworkWired => &mut self.network_wired,
            DeviceVisibilityKey::NetworkWireless => &mut self.network_wireless,
            DeviceVisibilityKey::NetworkVpn => &mut self.network_vpn,
            DeviceVisibilityKey::NetworkVirtual => &mut self.network_virtual,
            DeviceVisibilityKey::NetworkOther => &mut self.network_other,
            DeviceVisibilityKey::Gpus => &mut self.gpus,
        };
        if *target == visible {
            false
        } else {
            *target = visible;
            true
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_config_device_visibility_tests.rs"]
mod tests;
