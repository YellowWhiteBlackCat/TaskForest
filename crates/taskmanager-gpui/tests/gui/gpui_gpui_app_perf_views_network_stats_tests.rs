use super::network_stats;
use taskmanager_application::i18n;
use taskmanager_core::core::metrics::NetworkMetrics;
use taskmanager_core::core::units::UnitPreferences;
use taskmanager_test_support::NetworkMetricsFixtureBuilder;

fn labels_of(metrics: &NetworkMetrics) -> Vec<String> {
    network_stats(metrics, false, UnitPreferences::default())
        .iter()
        .map(|row| row.label().to_owned())
        .collect()
}

/// Address and link-speed facts that do not exist on the host are OMITTED
/// (求同存异): a default adapter with no addresses and no negotiated link
/// renders neither the IPv4/IPv6/MAC rows nor the link/utilization pair —
/// no permanent dash placeholders.
#[test]
fn absent_address_and_link_facts_omit_their_rows() {
    let metrics = NetworkMetrics::default();
    let labels = labels_of(&metrics);
    for absent in ["net.ipv4", "net.ipv6", "net.mac", "net.link"] {
        assert!(
            !labels.iter().any(|label| label == i18n::t(absent)),
            "{absent} row must be omitted without data, got {labels:?}"
        );
    }
    assert!(
        !labels
            .iter()
            .any(|label| label == i18n::t("common.utilization")),
        "utilization is keyed on link speed, got {labels:?}"
    );
}

/// Rate rows survive a first-sample gap as explicit `None` values (the
/// panel renders the shared dash), and existing facts render with values.
#[test]
fn rate_rows_keep_none_for_first_sample_gaps() {
    use taskmanager_core::core::metrics::{NetworkScalarObservations, ScalarObservation};
    let scalar_observations = NetworkScalarObservations {
        link_speed_mbps: ScalarObservation::available(1_000, 0),
        ..NetworkScalarObservations::default()
    };
    let metrics = NetworkMetricsFixtureBuilder::new()
        .ipv4_addr(Some("192.0.2.10".into()))
        .scalar_observations(scalar_observations)
        .build();
    let rows = network_stats(&metrics, false, UnitPreferences::default());
    let find = |key: &'static str| {
        rows.iter()
            .find(|row| row.label() == i18n::t(key))
            .unwrap_or_else(|| panic!("{key} row must exist"))
    };
    assert_eq!(find("net.receive").value(), None, "first sample is a gap");
    assert_eq!(
        find("net.ipv4").value(),
        Some("192.0.2.10"),
        "an existing address renders"
    );
    assert_eq!(
        find("net.link").value(),
        Some("1000 Mbps"),
        "a negotiated link speed renders"
    );
    assert_eq!(
        find("common.utilization").value(),
        None,
        "utilization stays an honest gap until sampled"
    );
}

#[test]
fn mtu_and_queue_rows_keep_measured_zero_visible() {
    use taskmanager_core::core::metrics::{
        NetworkAdapterType, NetworkScalarObservations, ScalarObservation,
    };
    let metrics = NetworkMetricsFixtureBuilder::new()
        .scalar_observations(NetworkScalarObservations {
            mtu_bytes: ScalarObservation::available(9000, 1),
            tx_queue_len: ScalarObservation::available(0, 1),
            ..Default::default()
        })
        .adapter_type(NetworkAdapterType::Ethernet)
        .build();
    let rows = network_stats(&metrics, false, UnitPreferences::default());
    let find = |key: &'static str| {
        rows.iter()
            .find(|row| row.label() == i18n::t(key))
            .unwrap_or_else(|| panic!("{key} row must exist"))
    };
    assert_eq!(find("net.mtu").value(), Some("9000 B"));
    assert_eq!(find("net.tx_queue").value(), Some("0"));
}

#[test]
fn packet_error_rows_are_omitted_until_a_counter_is_observed() {
    use taskmanager_core::core::metrics::{NetworkScalarObservations, ScalarObservation};
    let metrics = NetworkMetricsFixtureBuilder::new()
        .scalar_observations(NetworkScalarObservations {
            rx_drops: ScalarObservation::available(3, 1),
            tx_errors: ScalarObservation::available(1, 1),
            ..Default::default()
        })
        .build();
    let rows = network_stats(&metrics, false, UnitPreferences::default());
    let find = |key: &'static str| {
        rows.iter()
            .find(|row| row.label() == i18n::t(key))
            .unwrap_or_else(|| panic!("{key} row must exist"))
    };
    assert_eq!(find("net.drops").value(), Some("3 / —"));
    assert_eq!(find("net.errors").value(), Some("— / 1"));
    assert!(
        !rows
            .iter()
            .any(|row| row.label() == i18n::t("net.overruns"))
    );
}
