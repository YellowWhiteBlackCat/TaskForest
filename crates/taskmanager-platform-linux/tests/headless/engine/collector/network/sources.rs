use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

#[test]
fn wireless_parser_preserves_unknown_zero_as_no_measurement() {
    let parsed = parse_proc_wireless(
        "Inter-| sta\n face |\n wlp2s0: 0000 70. -42. -256\n wlan1: 0000 0. 0. 0\n",
    );

    assert_eq!(parsed.signals.get("wlp2s0"), Some(&-42));
    assert!(!parsed.signals.contains_key("wlan1"));
    assert_eq!(parsed.malformed_rows, 0);
}

#[test]
fn malformed_wireless_level_is_not_reported_as_zero_success() {
    let parsed = parse_proc_wireless("wlp2s0: 0000 70. unavailable 0\n");

    assert!(parsed.signals.is_empty());
    assert_eq!(parsed.malformed_rows, 1);
}

#[test]
fn iw_parser_distinguishes_association_from_valid_disconnection() {
    assert_eq!(
        parse_iw_link("Connected to aa:bb\n\tSSID: Studio Network 5G\n"),
        IwLinkResult::Associated {
            bssid: None,
            ssid: "Studio Network 5G".to_owned(),
            signal_dbm: None,
            frequency_mhz: None,
            channel: None,
            rx_bitrate_mbps: None,
            tx_bitrate_mbps: None,
            protocol: None,
        }
    );
    assert_eq!(
        parse_iw_link("Not connected.\n"),
        IwLinkResult::NotAssociated
    );
}

#[test]
fn iw_parser_keeps_signal_past_ssid_line() {
    // The SSID is reported before `signal:`; the parser must scan the whole
    // block instead of returning at the SSID line (regression guard for the
    // discarded-signal bug — modern mac80211 exposes dBm only via `iw`).
    assert_eq!(
        parse_iw_link("Connected to aa:bb\n\tSSID: office\n\tfreq: 5180\n\tsignal: -52 dBm\n"),
        IwLinkResult::Associated {
            bssid: None,
            ssid: "office".to_owned(),
            signal_dbm: Some(-52),
            frequency_mhz: Some(5180),
            channel: Some(36),
            rx_bitrate_mbps: None,
            tx_bitrate_mbps: None,
            protocol: None,
        }
    );
}

#[test]
fn iw_parser_ceils_tx_bitrate_and_keeps_absence_as_none() {
    // "tx bitrate: 866.7 MBit/s" must be ceilinged to 867 Mbps for the
    // link_speed backfill; a block without a tx-bitrate line stays None so the
    // caller never fabricates a 0 Mbps fallback.
    assert_eq!(
        parse_iw_link(
            "Connected to aa:bb\n\tSSID: studio\n\ttx bitrate: 866.7 MBit/s\n\tsignal: -50 dBm\n"
        ),
        IwLinkResult::Associated {
            bssid: None,
            ssid: "studio".to_owned(),
            signal_dbm: Some(-50),
            frequency_mhz: None,
            channel: None,
            rx_bitrate_mbps: None,
            tx_bitrate_mbps: Some(867),
            protocol: None,
        }
    );
    // A whole-number rate is not off-by-one'd upward.
    assert_eq!(
        parse_iw_link("Connected to aa:bb\n\tSSID: studio\n\ttx bitrate: 1000.0 MBit/s\n"),
        IwLinkResult::Associated {
            bssid: None,
            ssid: "studio".to_owned(),
            signal_dbm: None,
            frequency_mhz: None,
            channel: None,
            rx_bitrate_mbps: None,
            tx_bitrate_mbps: Some(1000),
            protocol: None,
        }
    );
    // No tx-bitrate line → None (distinct from a real 0 Mbps measurement).
    let parsed = parse_iw_link("Connected to aa:bb\n\tSSID: studio\n\tsignal: -40 dBm\n");
    let IwLinkResult::Associated {
        tx_bitrate_mbps, ..
    } = parsed
    else {
        panic!("expected Associated");
    };
    assert_eq!(tx_bitrate_mbps, None);
}

#[test]
fn iw_parser_collects_wireless_link_details_without_fabricating_missing_fields() {
    assert_eq!(
        parse_iw_link(
            "Connected to 02:11:22:33:44:55\n\
             \tSSID: studio\n\
             \tfreq: 5220\n\
             \trx bitrate: 2401.9 MBit/s EHT-MCS 13\n\
             \ttx bitrate: 4800.0 MBit/s EHT-MCS 13\n"
        ),
        IwLinkResult::Associated {
            bssid: Some("02:11:22:33:44:55".to_owned()),
            ssid: "studio".to_owned(),
            signal_dbm: None,
            frequency_mhz: Some(5220),
            channel: Some(44),
            rx_bitrate_mbps: Some(2402),
            tx_bitrate_mbps: Some(4800),
            protocol: Some("802.11be (Wi-Fi 7)"),
        }
    );
}

#[test]
fn iw_info_parser_keeps_channel_and_frequency_typed() {
    assert_eq!(
        parse_iw_info("Interface wlan0\n\tchannel 44 (5220 MHz), width: 80 MHz\n"),
        Some((44, 5220))
    );
    assert_eq!(parse_iw_info("Interface wlan0\n\ttype managed\n"), None);
}

#[test]
fn iw_failure_is_partial_only_when_another_interface_succeeded() {
    let partial = summarize_iw_results(vec![
        (
            Arc::from("wlan0"),
            IwLinkResult::Associated {
                bssid: None,
                ssid: "office".to_owned(),
                signal_dbm: None,
                frequency_mhz: None,
                channel: None,
                rx_bitrate_mbps: None,
                tx_bitrate_mbps: None,
                protocol: None,
            },
        ),
        (
            Arc::from("wlan1"),
            IwLinkResult::Failed(FailureKind::TimedOut),
        ),
    ]);
    let missing = summarize_iw_results(vec![(
        Arc::from("wlan0"),
        IwLinkResult::Failed(FailureKind::MissingDependency),
    )]);

    assert_eq!(
        partial.outcome,
        SourceOutcome::Partial(FailureKind::TimedOut)
    );
    assert_eq!(
        missing.outcome,
        SourceOutcome::Unavailable(FailureKind::MissingDependency)
    );
}

#[test]
fn io_errors_keep_permission_and_absence_distinct() {
    assert_eq!(
        io_failure(&io::Error::from(io::ErrorKind::PermissionDenied)),
        FailureKind::PermissionDenied
    );
    assert_eq!(
        io_failure(&io::Error::from(io::ErrorKind::NotFound)),
        FailureKind::Unsupported
    );
    assert_eq!(
        command_spawn_failure(&io::Error::from(io::ErrorKind::NotFound)),
        FailureKind::MissingDependency
    );
    assert_eq!(
        iw_output_failure("command failed: Operation not permitted (-1)"),
        FailureKind::PermissionDenied
    );
    assert_eq!(
        iw_output_failure("command failed: No such device (-19)"),
        FailureKind::IdentityChanged
    );
}

#[test]
fn sysfs_directory_success_is_independent_of_attribute_failure() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let root = crate::test_support::repo_temp_dir().join(format!(
        "taskmanager-network-sysfs-{}-{unique}",
        std::process::id()
    ));
    let interface = root.join("enp1s0");
    fs::create_dir_all(&interface).unwrap();

    let observed = read_sysfs_inventory(&root, 1_000);

    assert_eq!(observed.discovery_outcome, SourceOutcome::Available);
    assert_eq!(
        observed.metadata_outcome,
        SourceOutcome::Unavailable(FailureKind::Unsupported)
    );
    assert_eq!(observed.value.len(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn sysfs_inventory_keeps_virtual_and_vpn_entries_for_typed_visibility_filters() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let root = crate::test_support::repo_temp_dir().join(format!(
        "taskmanager-network-categories-{}-{unique}",
        std::process::id()
    ));
    for name in ["enp1s0", "docker0", "tun0"] {
        fs::create_dir_all(root.join(name)).unwrap();
    }

    let observed = read_sysfs_inventory(&root, 1_000);

    assert_eq!(observed.discovery_outcome, SourceOutcome::Available);
    assert_eq!(observed.value.len(), 3);
    assert!(
        observed
            .value
            .iter()
            .any(|interface| interface.name.as_ref() == "docker0")
    );
    assert!(
        observed
            .value
            .iter()
            .any(|interface| interface.name.as_ref() == "tun0")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn zero_link_speed_is_temporary_unavailability_not_a_real_capacity() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let path = crate::test_support::repo_temp_dir().join(format!(
        "taskmanager-network-speed-{}-{unique}",
        std::process::id()
    ));
    fs::write(&path, "0\n").unwrap();

    let observed = read_link_speed(&path, 1_000);

    assert_eq!(
        observed.availability(),
        taskmanager_core::ScalarAvailability::Unavailable(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(observed.current_value(), None);
    fs::remove_file(path).unwrap();
}

#[test]
fn carrier_zero_is_a_current_down_link_not_missing_telemetry() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let path = crate::test_support::repo_temp_dir().join(format!(
        "taskmanager-network-carrier-{}-{unique}",
        std::process::id()
    ));
    fs::write(&path, "0\n").unwrap();

    let observed = read_link_up(&path, 1_000);

    assert_eq!(observed, ScalarObservation::available(false, 1_000));
    fs::remove_file(path).unwrap();
}
