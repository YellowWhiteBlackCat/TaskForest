//! Network-interface grouping folds for the shared presentation contract.
//!
//! Keeping this classification in its own module makes it harder for one
//! renderer to silently invent a virtual/physical taxonomy of its own.

use taskmanager_application::i18n;
use taskmanager_core::core::metrics::{NetworkAdapterType, NetworkMetrics};

/// Coarse network interface classification for grouped UI presentation
/// (physical vs virtual).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum NetworkInterfaceGroup {
    /// Physical network adapter (e.g. Ethernet, Wi-Fi, WWAN).
    Physical,
    /// Virtual or software network adapter (e.g. veth, bridge, tunnel).
    Virtual,
}

/// Alias for [`NetworkInterfaceGroup`].
pub type NetworkInterfaceKind = NetworkInterfaceGroup;

impl NetworkInterfaceGroup {
    /// Whether this interface group represents a physical adapter.
    #[must_use]
    pub const fn is_physical(self) -> bool {
        matches!(self, Self::Physical)
    }

    /// Whether this interface group represents a virtual adapter.
    #[must_use]
    pub const fn is_virtual(self) -> bool {
        matches!(self, Self::Virtual)
    }

    /// Stable non-localized key for UI grouping or preference identification.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Physical => "physical",
            Self::Virtual => "virtual",
        }
    }

    /// Localized display label for grouped section headers.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Physical => i18n::t("common.physical"),
            Self::Virtual => i18n::t("settings.network_virtual"),
        }
    }
}

/// Check whether a network interface name matches well-known virtual or
/// software-device patterns.
#[must_use]
pub fn is_virtual_network_interface_name(name: &str) -> bool {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return false;
    }

    if trimmed.eq_ignore_ascii_case("lo") || trimmed.eq_ignore_ascii_case("lo0") {
        return true;
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("veth")
        || lower.starts_with("docker")
        || lower.starts_with("virbr")
        || lower.starts_with("br-")
        || lower.starts_with("vnet")
        || lower.starts_with("tun")
        || lower.starts_with("tap")
        || lower.starts_with("vmnet")
        || lower.starts_with("vboxnet")
        || lower.starts_with("dummy")
        || lower.starts_with("flannel")
        || lower.starts_with("cni")
        || lower.starts_with("cali")
        || lower.starts_with("cilium")
        || lower.starts_with("tailscale")
        || lower.starts_with("utun")
        || lower.starts_with("vethernet")
        || lower.starts_with("bridge")
    {
        return true;
    }

    if lower.starts_with("wg-")
        || (lower.starts_with("wg")
            && lower.len() > 2
            && lower[2..].chars().all(|c| c.is_ascii_digit()))
    {
        return true;
    }

    if lower.starts_with("br")
        && lower.len() > 2
        && lower[2..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
    {
        return true;
    }

    if lower.starts_with("zt") && lower.len() > 2 {
        return true;
    }

    lower.contains("virtual")
        || lower.contains("hyper-v")
        || lower.contains("vmware")
        || lower.contains("virtualbox")
        || lower.contains("tap-windows")
        || lower.contains("loopback")
        || lower.contains("pseudo-interface")
}

/// Check whether a network interface name matches a virtual/software pattern.
#[must_use]
pub fn is_virtual_network_interface(name: &str) -> bool {
    is_virtual_network_interface_name(name)
}

/// Check whether a network interface name appears to be physical.
#[must_use]
pub fn is_physical_network_interface_name(name: &str) -> bool {
    !is_virtual_network_interface_name(name)
}

/// Check whether a network interface name appears to be physical.
#[must_use]
pub fn is_physical_network_interface(name: &str) -> bool {
    is_physical_network_interface_name(name)
}

/// Classify a network interface name as physical or virtual.
#[must_use]
pub fn classify_network_interface_name(name: &str) -> NetworkInterfaceGroup {
    if is_virtual_network_interface_name(name) {
        NetworkInterfaceGroup::Virtual
    } else {
        NetworkInterfaceGroup::Physical
    }
}

/// Classify a network interface name as physical or virtual.
#[must_use]
pub fn classify_network_interface(name: &str) -> NetworkInterfaceGroup {
    classify_network_interface_name(name)
}

/// Classify a typed network device for grouped presentation.
#[must_use]
pub fn classify_network_device(nic: &NetworkMetrics) -> NetworkInterfaceGroup {
    match nic.adapter_type() {
        NetworkAdapterType::Virtual | NetworkAdapterType::Loopback | NetworkAdapterType::Vpn => {
            NetworkInterfaceGroup::Virtual
        }
        NetworkAdapterType::WiFi => NetworkInterfaceGroup::Physical,
        NetworkAdapterType::Ethernet | NetworkAdapterType::Other | NetworkAdapterType::Unknown => {
            let name = if nic.interface_name.is_empty() {
                nic.device_id.as_ref()
            } else {
                nic.interface_name.as_ref()
            };
            classify_network_interface_name(name)
        }
    }
}

/// Check whether a typed network device is virtual.
#[must_use]
pub fn is_virtual_network_device(nic: &NetworkMetrics) -> bool {
    classify_network_device(nic).is_virtual()
}

/// Check whether a typed network device is physical.
#[must_use]
pub fn is_physical_network_device(nic: &NetworkMetrics) -> bool {
    classify_network_device(nic).is_physical()
}

/// Shorthand for [`is_virtual_network_device`].
#[must_use]
pub fn is_virtual_nic(nic: &NetworkMetrics) -> bool {
    is_virtual_network_device(nic)
}

/// Shorthand for [`is_physical_network_device`].
#[must_use]
pub fn is_physical_nic(nic: &NetworkMetrics) -> bool {
    is_physical_network_device(nic)
}

/// Partition typed network devices into physical and virtual collections.
#[must_use]
pub fn group_network_devices<'a>(
    devices: impl IntoIterator<Item = &'a NetworkMetrics>,
) -> (Vec<&'a NetworkMetrics>, Vec<&'a NetworkMetrics>) {
    let mut physical = Vec::new();
    let mut virtual_devs = Vec::new();
    for nic in devices {
        if is_virtual_network_device(nic) {
            virtual_devs.push(nic);
        } else {
            physical.push(nic);
        }
    }
    (physical, virtual_devs)
}
