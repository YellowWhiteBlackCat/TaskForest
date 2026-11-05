//! Renderer-edge network category labels shared by the sidebar and stats rail.

use taskforest_core::core::metrics::NetworkAdapterType;
use taskmanager_application::i18n::t;

pub(crate) fn network_category_label(adapter_type: NetworkAdapterType) -> &'static str {
    match adapter_type {
        NetworkAdapterType::Ethernet => t("settings.network_wired"),
        NetworkAdapterType::WiFi => t("settings.network_wireless"),
        NetworkAdapterType::Vpn => t("settings.network_vpn"),
        NetworkAdapterType::Virtual => t("settings.network_virtual"),
        NetworkAdapterType::Unknown | NetworkAdapterType::Loopback | NetworkAdapterType::Other => {
            t("settings.network_other")
        }
    }
}
