//! Small pure folds used by the GPUI process-insights cards.

use taskmanager_application::i18n;
use taskmanager_core::core::process_telemetry::{
    ConnectionAddressFamily, ConnectionTransport, LimitValue, ProcessConnection,
};
use taskmanager_core::core::units::{QuantityFamily, UnitPreferences};

pub(crate) fn format_connection(connection: &ProcessConnection) -> String {
    let transport = match (&connection.transport, &connection.family) {
        (ConnectionTransport::Tcp, ConnectionAddressFamily::Ipv6) => "TCP6".to_string(),
        (ConnectionTransport::Udp, ConnectionAddressFamily::Ipv6) => "UDP6".to_string(),
        _ => connection.transport.to_string(),
    };
    let scope = if connection.is_loopback() {
        "LOOPBACK"
    } else {
        "EXTERNAL"
    };
    let rtt = connection
        .rtt_ms
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map_or_else(String::new, |value| {
            format!(" · {} {value:.1} ms", i18n::t("net.rtt"))
        });
    format!(
        "{transport} [{} · {scope}]  {} → {}{rtt}",
        connection.state, connection.local, connection.remote
    )
}

pub(super) fn format_bytes(units: UnitPreferences, bytes: u64) -> String {
    units.format_quantity(bytes, QuantityFamily::Memory, false)
}

pub(super) fn format_rate(units: UnitPreferences, value: Option<u64>, unavailable: &str) -> String {
    value
        .map(|value| units.format_quantity(value, QuantityFamily::Network, true))
        .unwrap_or_else(|| unavailable.to_string())
}

pub(super) fn format_limit(
    value: LimitValue,
    unlimited: &str,
    format_value: impl FnOnce(u64) -> String,
) -> String {
    match value {
        LimitValue::Unlimited => unlimited.to_string(),
        LimitValue::Value(value) => format_value(value),
    }
}

pub(super) fn format_pair(
    current: Option<String>,
    maximum: Option<String>,
    unknown: &str,
) -> String {
    match (current, maximum) {
        (Some(current), Some(maximum)) => format!("{current} / {maximum}"),
        (Some(current), None) => format!("{current} / {unknown}"),
        (None, Some(maximum)) => format!("{unknown} / {maximum}"),
        (None, None) => unknown.to_string(),
    }
}

pub(super) fn isolation_label(
    kind: &taskmanager_core::core::process_telemetry::IsolationKind,
) -> &'static str {
    use taskmanager_core::core::process_telemetry::IsolationKind;
    match kind {
        IsolationKind::Docker => "Docker",
        IsolationKind::Podman => "Podman",
        IsolationKind::Kubernetes => "Kubernetes",
        IsolationKind::Lxc => "LXC",
        IsolationKind::SystemdNspawn => "systemd-nspawn",
        IsolationKind::Flatpak => "Flatpak",
        IsolationKind::Snap => "Snap",
        IsolationKind::Wsl => "WSL",
        IsolationKind::OtherContainer => "Container",
    }
}
