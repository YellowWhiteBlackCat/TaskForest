use super::*;

#[test]
fn loopback_filter_does_not_hide_named_windows_adapters() {
    assert!(is_loopback("lo"));
    assert!(is_loopback("Loopback Pseudo-Interface 1"));
    assert!(is_loopback("localhost"));
    assert!(!is_loopback("Local Area Connection"));
    assert!(!is_loopback("Ethernet"));
}

#[test]
fn windows_wifi_metadata_is_unavailable_without_location_access() {
    let wireless = wireless_observations_for(NetworkAdapterType::WiFi, 100);
    assert_eq!(
        wireless.ssid.availability(),
        taskmanager_core::ScalarAvailability::Unavailable(FailureKind::Unsupported)
    );
    assert_eq!(wireless.ssid.current_value(), None);
    assert_eq!(wireless.signal_dbm.current_value(), None);
    assert!(!wireless.ssid.is_current_not_applicable());

    let wired = wireless_observations_for(NetworkAdapterType::Ethernet, 100);
    assert!(wired.ssid.is_current_not_applicable());
}

#[test]
fn operational_state_is_not_guessed_for_unknown_values() {
    assert_eq!(
        operational_state_link_up(sysinfo::InterfaceOperationalState::Up),
        Some(true)
    );
    assert_eq!(
        operational_state_link_up(sysinfo::InterfaceOperationalState::Down),
        Some(false)
    );
    assert_eq!(
        operational_state_link_up(sysinfo::InterfaceOperationalState::Unknown),
        None
    );
}

#[test]
fn link_capacity_uses_the_larger_native_direction() {
    let adapter = taskmanager_windows_api::WindowsNetworkAdapter {
        name: "Ethernet".to_owned(),
        description: "Intel Ethernet".to_owned(),
        adapter_type: taskmanager_windows_api::WindowsAdapterType::Ethernet,
        receive_link_speed_bps: Some(1_000_000_000),
        transmit_link_speed_bps: Some(100_000_000),
        link_up: Some(true),
    };
    assert_eq!(adapter_link_speed_mbps(&adapter), Some(1_000));
}

#[test]
fn link_utilization_pct_mirrors_linux_formula_and_clamps_to_100() {
    // 125 MB/s aggregate (rx+tx) on a 1 Gbps link == 100% exactly at the
    // saturation boundary (1_000_000_000 bits/s).
    assert_eq!(link_utilization_pct(62_500_000, 62_500_000, 1_000), 100.0);
    // 12.5 MB/s aggregate on a 1 Gbps link == 10%.
    assert_eq!(link_utilization_pct(10_000_000, 2_500_000, 1_000), 10.0);
    // 6.25 MB/s one-way on a 100 Mbps link == 50%.
    assert_eq!(link_utilization_pct(6_250_000, 0, 100), 50.0);
    // Traffic exceeding capacity saturates — never reports >100.
    assert_eq!(link_utilization_pct(200_000_000, 200_000_000, 1_000), 100.0);
}

#[test]
fn link_utilization_pct_zero_traffic_is_honest_measured_zero() {
    // No rx+tx bytes against a known link is a real measured 0.0, NOT
    // unavailable — parity with the Linux idle-second utilization receipt
    // (counters/tests.rs: first_sample...second_idle_sample_is_real_zero).
    assert_eq!(link_utilization_pct(0, 0, 1_000), 0.0);
}

#[test]
fn derive_utilization_pct_is_available_when_link_speed_is_known() {
    // A future native adapter query can provide a known link speed; the
    // pure projection remains tested independently of that boundary.
    let link_speed = ScalarObservation::available(1_000, 100);
    let utilization = derive_utilization_pct(
        ScalarObservation::available(10_000_000, 100),
        ScalarObservation::available(2_500_000, 100),
        link_speed,
        100,
    );
    assert_eq!(utilization, ScalarObservation::available(10.0, 100));
}

#[test]
fn derive_utilization_pct_rides_link_speed_failure_when_unknown() {
    // No native adapter query is registered yet: link_speed degrades to
    // Unsupported -> utilization rides the same failure, never a
    // fabricated 0.
    let link_speed = ScalarObservation::unavailable(FailureKind::Unsupported);
    let utilization = derive_utilization_pct(
        ScalarObservation::available(99_999_999, 100),
        ScalarObservation::available(99_999_999, 100),
        link_speed,
        100,
    );
    assert_eq!(
        utilization,
        ScalarObservation::unavailable(FailureKind::Unsupported)
    );
    assert_eq!(utilization.current_value(), None);
}

#[test]
fn derive_utilization_pct_never_turns_a_rate_failure_into_zero() {
    let utilization = derive_utilization_pct(
        ScalarObservation::unavailable(FailureKind::PermissionDenied),
        ScalarObservation::available(0, 100),
        ScalarObservation::available(1_000, 100),
        100,
    );

    assert_eq!(
        utilization.availability(),
        taskmanager_core::ScalarAvailability::Unavailable(FailureKind::PermissionDenied)
    );
    assert_eq!(utilization.current_value(), None);
}

#[test]
fn windows_network_provider_real_refresh() {
    let mut provider = WinNetworkTelemetryProvider::new();
    let obs = provider.refresh(100).expect("network observation");
    let metrics = obs.current_value().expect("metrics present");
    assert!(!metrics.is_empty());
    for metric in metrics {
        eprintln!(
            "NIC: name='{}', type={:?}, speed={:?} Mbps, up={:?}, is_wifi={}, ssid={:?}, signal={:?} dBm",
            metric.interface_name,
            metric.adapter_type(),
            metric.current_link_speed_mbps(),
            metric.current_link_up(),
            metric.adapter_type() == NetworkAdapterType::WiFi,
            metric.current_ssid(),
            metric.current_signal_dbm()
        );
    }
}
