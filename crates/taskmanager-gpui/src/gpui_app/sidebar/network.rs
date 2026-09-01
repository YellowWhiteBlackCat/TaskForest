//! Renderer-edge network category labels and visibility classification helpers.

use taskmanager_application::i18n;
use taskmanager_core::core::metrics::{NetworkAdapterType, NetworkMetrics};
use taskmanager_shell::presentation::optional_wifi_signal_quality_percent;

/// Per-network-category visibility policy projected from the Settings/RootView
/// preference state. This is presentation policy only: providers publish all
/// discovered interfaces, and the sidebar/compact strip decide what to render.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NetworkVisibility {
    pub all: bool,
    pub wired: bool,
    pub wireless: bool,
    pub vpn: bool,
    pub virtual_devices: bool,
    pub other: bool,
}

impl NetworkVisibility {
    #[must_use]
    pub const fn allows(self, adapter_type: NetworkAdapterType) -> bool {
        self.all
            && match adapter_type {
                NetworkAdapterType::Ethernet => self.wired,
                NetworkAdapterType::WiFi => self.wireless,
                NetworkAdapterType::Vpn => self.vpn,
                NetworkAdapterType::Virtual => self.virtual_devices,
                NetworkAdapterType::Unknown
                | NetworkAdapterType::Loopback
                | NetworkAdapterType::Other => self.other,
            }
    }
}

pub(crate) fn network_category_label(adapter_type: NetworkAdapterType) -> &'static str {
    match adapter_type {
        NetworkAdapterType::Ethernet => i18n::t("settings.network_wired"),
        NetworkAdapterType::WiFi => i18n::t("settings.network_wireless"),
        NetworkAdapterType::Vpn => i18n::t("settings.network_vpn"),
        NetworkAdapterType::Virtual => i18n::t("settings.network_virtual"),
        NetworkAdapterType::Unknown | NetworkAdapterType::Loopback | NetworkAdapterType::Other => {
            i18n::t("settings.network_other")
        }
    }
}

/// Second caption line for a NIC. Wireless surfaces SSID · signal% · link Mbps;
/// wired surfaces the interface name · link Mbps. Each piece is gated on its
/// source being present, so absent observations are omitted rather than printing a
/// dangling separator. Signal dBm→% reads the shared -90..-30 → 0..100 fold the
/// network detail view (`perf_views`) renders, so the two views can never
/// disagree; when nothing is available the line falls back to the IPv4 address
/// (or interface name).
pub(crate) fn nic_caption_line2(n: &NetworkMetrics) -> String {
    let mut parts: Vec<String> = Vec::new();
    if n.adapter_type() == NetworkAdapterType::WiFi {
        if let Some(ssid) = n.current_ssid()
            && !ssid.is_empty()
        {
            parts.push(ssid.to_string());
        }
        if let Some(pct) = optional_wifi_signal_quality_percent(n.current_signal_dbm()) {
            parts.push(format!("{pct:.0}%"));
        }
    } else {
        parts.push(n.interface_name.as_ref().to_owned());
    }
    if let Some(link) = n.current_link_speed_mbps() {
        parts.push(format!("{link} Mbps"));
    }
    if parts.is_empty() {
        n.ipv4_addr
            .as_deref()
            .map(str::to_owned)
            .unwrap_or_else(|| n.interface_name.as_ref().to_owned())
    } else {
        parts.join("  ·  ")
    }
}
