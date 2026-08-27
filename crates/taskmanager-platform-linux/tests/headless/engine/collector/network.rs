use super::*;
use taskmanager_core::DeviceRefreshOutcome;

fn interface(name: &str, arp_type: u64) -> SysfsInterface {
    SysfsInterface {
        stable_id: Arc::from("net:mac:aa:bb:cc:dd:ee:ff"),
        name: Arc::from(name),
        arp_type: Some(arp_type),
        mac_addr: Some(Arc::from("aa:bb:cc:dd:ee:ff")),
        link_speed: ScalarObservation::available(1_000, 1),
        link_up: ScalarObservation::available(true, 1),
        driver: Some(Arc::from("fixture")),
        adapter: Some(Arc::from("fixture adapter")),
    }
}

fn available_counters(value: HashMap<Arc<str>, CounterValues>) -> CounterObservation {
    CounterObservation {
        current_count: value.len(),
        value,
        outcome: SourceOutcome::Available,
    }
}

#[test]
fn enrichment_failures_do_not_remove_a_sysfs_discovered_nic() {
    let inventory = ResolvedInventory {
        interfaces: vec![interface("wlan0", 1)],
        discovered_devices: vec![DeviceId::new("net:mac:aa:bb:cc:dd:ee:ff")],
        outcome: SourceOutcome::Available,
        fresh_count: 1,
        metadata_outcome: SourceOutcome::Unavailable(FailureKind::PermissionDenied),
        metadata_item_count: 0,
        fresh_interfaces: HashSet::from([Arc::from("wlan0")]),
    };
    let snapshot = assemble_snapshot(
        inventory,
        available_counters(HashMap::from([(
            Arc::from("wlan0"),
            CounterValues {
                rx_rate: ScalarObservation::available(0, 1_000),
                tx_rate: ScalarObservation::available(0, 1_000),
                utilization: ScalarObservation::available(0.0, 1_000),
                ..Default::default()
            },
        )])),
        SourceObservation {
            value: HashMap::new(),
            outcome: SourceOutcome::Unavailable(FailureKind::PermissionDenied),
        },
        SourceObservation {
            value: HashMap::new(),
            outcome: SourceOutcome::Unavailable(FailureKind::Unsupported),
        },
        SourceObservation {
            value: HashMap::new(),
            outcome: SourceOutcome::Unavailable(FailureKind::MissingDependency),
        },
        1_000,
    );

    assert_eq!(snapshot.value.len(), 1);
    assert_eq!(snapshot.discovered_devices().len(), 1);
    assert_eq!(&*snapshot.value[0].interface_name, "wlan0");
    assert_eq!(snapshot.value[0].current_ssid(), None);
    assert_eq!(
        snapshot.value[0]
            .wireless_observations()
            .association
            .availability(),
        taskmanager_core::ScalarAvailability::Unavailable(FailureKind::MissingDependency)
    );
    assert_eq!(
        DeviceRefreshOutcome::from_discovery_outcome(snapshot.discovery().outcome),
        DeviceRefreshOutcome::Complete
    );
    assert!(snapshot.enrichments.iter().any(|source| {
        source.provider.as_str() == IW_PROVIDER
            && source.outcome == SourceOutcome::Unavailable(FailureKind::MissingDependency)
    }));
    assert!(snapshot.enrichments.iter().any(|source| {
        source.provider.as_str() == SYSFS_METADATA_PROVIDER
            && source.outcome == SourceOutcome::Unavailable(FailureKind::PermissionDenied)
    }));
}

#[test]
fn failed_sysfs_refresh_retains_cache_but_cannot_confirm_absence() {
    let mut state = NetworkCollectionState::default();
    let first = resolve_inventory(
        &mut state,
        SysfsInventoryObservation {
            value: vec![interface("enp1s0", 1)],
            discovery_outcome: SourceOutcome::Available,
            metadata_outcome: SourceOutcome::Available,
            metadata_item_count: 1,
        },
    );
    let failed = resolve_inventory(
        &mut state,
        SysfsInventoryObservation {
            value: Vec::new(),
            discovery_outcome: SourceOutcome::Unavailable(FailureKind::PermissionDenied),
            metadata_outcome: SourceOutcome::Unavailable(FailureKind::PermissionDenied),
            metadata_item_count: 0,
        },
    );

    assert_eq!(first.interfaces.len(), 1);
    assert_eq!(failed.interfaces.len(), 1);
    assert!(failed.discovered_devices.is_empty());
    assert_eq!(failed.interfaces[0].name.as_ref(), "enp1s0");
    assert_eq!(
        failed.interfaces[0].link_speed.availability(),
        taskmanager_core::ScalarAvailability::Stale(FailureKind::PermissionDenied)
    );
    assert_eq!(
        failed.interfaces[0].link_up.availability(),
        taskmanager_core::ScalarAvailability::Stale(FailureKind::PermissionDenied)
    );
    assert_eq!(
        DeviceRefreshOutcome::from_discovery_outcome(failed.outcome),
        DeviceRefreshOutcome::Unavailable(taskmanager_core::DeviceStatus::PermissionDenied)
    );
}

#[test]
fn temporary_mac_failure_does_not_downgrade_a_fresh_nic_identity() {
    let mut state = NetworkCollectionState::default();
    let first = resolve_inventory(
        &mut state,
        SysfsInventoryObservation {
            value: vec![interface("enp1s0", 1)],
            discovery_outcome: SourceOutcome::Available,
            metadata_outcome: SourceOutcome::Available,
            metadata_item_count: 1,
        },
    );
    let mut without_mac = interface("enp1s0", 1);
    without_mac.mac_addr = None;
    let degraded = resolve_inventory(
        &mut state,
        SysfsInventoryObservation {
            value: vec![without_mac],
            discovery_outcome: SourceOutcome::Available,
            metadata_outcome: SourceOutcome::Partial(FailureKind::PermissionDenied),
            metadata_item_count: 1,
        },
    );

    assert_eq!(degraded.discovered_devices, first.discovered_devices);
    assert_eq!(
        degraded.interfaces[0].mac_addr.as_deref(),
        Some("aa:bb:cc:dd:ee:ff")
    );
    assert_eq!(
        degraded.metadata_outcome,
        SourceOutcome::Partial(FailureKind::PermissionDenied)
    );
}

#[test]
fn discovery_failure_mapping_is_typed_and_lossless() {
    assert_eq!(
        source_failure(SourceOutcome::Unavailable(FailureKind::PermissionDenied)),
        Some(FailureKind::PermissionDenied)
    );
    assert_eq!(
        source_failure(SourceOutcome::Partial(FailureKind::ProviderFault)),
        Some(FailureKind::ProviderFault)
    );
    assert_eq!(source_failure(SourceOutcome::Available), None);
}

#[test]
fn adapter_classification_is_independent_of_counter_or_iw_availability() {
    assert_eq!(
        classify_adapter_type("wlp2s0", Some(1)),
        NetworkAdapterType::WiFi
    );
    assert_eq!(
        classify_adapter_type("enp3s0", Some(1)),
        NetworkAdapterType::Ethernet
    );
    assert_eq!(
        classify_adapter_type("eth0", Some(1)),
        NetworkAdapterType::Ethernet
    );
    assert_eq!(
        classify_adapter_type("mesh0", Some(803)),
        NetworkAdapterType::WiFi
    );
    assert_eq!(
        classify_adapter_type("tun0", Some(1)),
        NetworkAdapterType::Vpn
    );
    assert_eq!(
        classify_adapter_type("wg0", Some(1)),
        NetworkAdapterType::Vpn
    );
    assert_eq!(
        classify_adapter_type("docker0", Some(1)),
        NetworkAdapterType::Virtual
    );
    assert_eq!(
        classify_adapter_type("lo", Some(772)),
        NetworkAdapterType::Loopback
    );
    assert_eq!(
        classify_adapter_type("bluetooth0", Some(1)),
        NetworkAdapterType::Other
    );
}

#[test]
fn wired_and_unassociated_wireless_fields_have_explicit_optional_states() {
    let wired = assemble_wireless_observations(
        false,
        "enp1s0",
        &mut SourceObservation {
            value: HashMap::new(),
            outcome: SourceOutcome::Empty,
        },
        &mut SourceObservation {
            value: HashMap::new(),
            outcome: SourceOutcome::Empty,
        },
        10,
    );
    let unassociated = assemble_wireless_observations(
        true,
        "wlan0",
        &mut SourceObservation {
            value: HashMap::from([("wlan0".to_owned(), -42)]),
            outcome: SourceOutcome::Available,
        },
        &mut SourceObservation {
            value: HashMap::from([(Arc::from("wlan0"), IwLinkResult::NotAssociated)]),
            outcome: SourceOutcome::Available,
        },
        20,
    );

    assert!(wired.association.is_current_not_applicable());
    assert!(wired.ssid.is_current_not_applicable());
    assert!(wired.signal_dbm.is_current_not_applicable());
    assert!(unassociated.association.is_current_absent());
    assert!(unassociated.ssid.is_current_absent());
    assert!(
        unassociated.signal_dbm.is_current_absent(),
        "confirmed unassociation wins over a stale proc signal row"
    );
}

#[test]
fn typed_observation_retention_is_stable_id_scoped_and_resets_on_lifecycle_boundary() {
    let device_id = "net:mac:aa:bb:cc:dd:ee:ff";
    let mut state = NetworkObservationState::default();
    let mut first = vec![
        taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
            .device_id(device_id.into())
            .interface_name("wlan0".into())
            .scalar_observations(NetworkScalarObservations {
                rx_bytes_per_sec: ScalarObservation::available(42, 10),
                ..Default::default()
            })
            .wireless_observations(NetworkWirelessObservations {
                association: OptionalObservation::present(true, 10),
                ssid: OptionalObservation::present("studio".into(), 10),
                signal_dbm: OptionalObservation::present(-50, 10),
                ..Default::default()
            })
            .build(),
    ];
    state.reconcile(&mut first);

    let mut renamed = vec![
        taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
            .device_id(device_id.into())
            .interface_name("wlp2s0".into())
            .scalar_observations(NetworkScalarObservations::unavailable(
                FailureKind::TemporarilyUnavailable,
            ))
            .wireless_observations(NetworkWirelessObservations::unavailable(
                FailureKind::TimedOut,
            ))
            .build(),
    ];
    state.reconcile(&mut renamed);

    assert_eq!(
        renamed[0]
            .scalar_observations()
            .rx_bytes_per_sec
            .availability(),
        taskmanager_core::ScalarAvailability::Stale(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(
        renamed[0].wireless_observations().ssid.last_known_state(),
        &taskmanager_core::OptionalObservationState::Present("studio".into())
    );
    assert_eq!(renamed[0].current_ssid(), None);

    state.reset_absent(&[DeviceId::new(device_id)]);
    let mut reattached = vec![
        taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
            .device_id(device_id.into())
            .interface_name("wlan0".into())
            .scalar_observations(NetworkScalarObservations::unavailable(
                FailureKind::TemporarilyUnavailable,
            ))
            .wireless_observations(NetworkWirelessObservations::unavailable(
                FailureKind::TimedOut,
            ))
            .build(),
    ];
    state.reconcile(&mut reattached);
    state.confirm_reappeared(&[DeviceId::new(device_id)]);
    assert_eq!(
        reattached[0]
            .scalar_observations()
            .rx_bytes_per_sec
            .availability(),
        taskmanager_core::ScalarAvailability::Unavailable(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(
        reattached[0]
            .wireless_observations()
            .ssid
            .last_known_state(),
        &taskmanager_core::OptionalObservationState::Unknown
    );

    let mut first_current_in_new_generation = vec![
        taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
            .device_id(device_id.into())
            .interface_name("wlan0".into())
            .scalar_observations(NetworkScalarObservations {
                rx_bytes_per_sec: ScalarObservation::available(7, 30),
                ..Default::default()
            })
            .build(),
    ];
    state.reconcile(&mut first_current_in_new_generation);
    let mut failed_again = vec![
        taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
            .device_id(device_id.into())
            .interface_name("wlan0".into())
            .scalar_observations(NetworkScalarObservations::unavailable(
                FailureKind::PermissionDenied,
            ))
            .build(),
    ];
    state.reconcile(&mut failed_again);
    assert_eq!(
        failed_again[0]
            .scalar_observations()
            .rx_bytes_per_sec
            .last_known_value(),
        Some(&7),
        "the first successful sample of the new generation remains retainable"
    );
}

#[test]
fn wifi_without_sysfs_speed_backfills_link_speed_and_utilization_from_iw_tx_bitrate() {
    // /sys/class/net/<wifi>/speed is absent on most mac80211 drivers, so the
    // counter path (which runs before iw is fetched) computed utilization
    // against a None link speed. When iw reports a tx bitrate, assemble_snapshot
    // must backfill link_speed_mbps AND recompute utilization so a WiFi adapter
    // shows a real capacity instead of "—".
    let mut wifi = interface("wlan0", 1);
    wifi.link_speed = ScalarObservation::unavailable(FailureKind::Unsupported);
    let inventory = ResolvedInventory {
        interfaces: vec![wifi],
        discovered_devices: vec![DeviceId::new("net:mac:aa:bb:cc:dd:ee:ff")],
        outcome: SourceOutcome::Available,
        fresh_count: 1,
        metadata_outcome: SourceOutcome::Available,
        metadata_item_count: 1,
        fresh_interfaces: HashSet::from([Arc::from("wlan0")]),
    };
    // The counter path saw the unavailable sysfs link_speed, so its utilization
    // is Unavailable; rx/tx rates themselves are current.
    let counters = available_counters(HashMap::from([(
        Arc::from("wlan0"),
        CounterValues {
            rx_rate: ScalarObservation::available(10_000_000, 1_000),
            tx_rate: ScalarObservation::available(5_000_000, 1_000),
            utilization: ScalarObservation::unavailable(FailureKind::Unsupported),
            ..Default::default()
        },
    )]));
    let iw = SourceObservation {
        value: HashMap::from([(
            Arc::from("wlan0"),
            IwLinkResult::Associated {
                bssid: Some("aa:bb:cc:dd:ee:ff".to_owned()),
                ssid: "studio".to_owned(),
                signal_dbm: Some(-50),
                frequency_mhz: Some(5180),
                channel: Some(36),
                rx_bitrate_mbps: Some(433),
                tx_bitrate_mbps: Some(867),
                protocol: Some("802.11ac (Wi-Fi 5)"),
            },
        )]),
        outcome: SourceOutcome::Available,
    };

    let snapshot = assemble_snapshot(
        inventory,
        counters,
        SourceObservation {
            value: HashMap::new(),
            outcome: SourceOutcome::Empty,
        },
        SourceObservation {
            value: HashMap::new(),
            outcome: SourceOutcome::Empty,
        },
        iw,
        1_000,
    );

    let metric = &snapshot.value[0];
    assert_eq!(
        metric.scalar_observations().link_speed_mbps.current_value(),
        Some(&867),
        "the iw tx bitrate (ceiled to u64 Mbps) backfills the missing sysfs speed"
    );
    let utilization = metric
        .scalar_observations()
        .utilization_pct
        .current_value()
        .copied();
    let pct = utilization.expect("utilization must be computable once a link speed flows in");
    // 15 MB/s (rx+tx) over 867 Mbps ≈ 13.84%.
    assert!(
        (13.0..=15.0).contains(&pct),
        "utilization {pct} should be ~13.8% of the backfilled 867 Mbps capacity"
    );
    assert_eq!(
        metric.current_ssid(),
        Some("studio"),
        "the wireless association path is unaffected by the link-speed backfill"
    );
    assert_eq!(metric.current_bssid(), Some("aa:bb:cc:dd:ee:ff"));
    assert_eq!(metric.current_frequency_mhz(), Some(5180));
    assert_eq!(metric.current_channel(), Some(36));
    assert_eq!(metric.current_rx_bitrate_mbps(), Some(433));
    assert_eq!(metric.current_tx_bitrate_mbps(), Some(867));
    assert_eq!(metric.current_protocol(), Some("802.11ac (Wi-Fi 5)"));
}

#[test]
fn wifi_without_sysfs_speed_and_without_iw_bitrate_keeps_typed_unavailable() {
    // No sysfs speed AND no iw tx bitrate: link_speed stays a typed Unavailable
    // (the sysfs state), and utilization stays Unavailable. None is never
    // fabricated as 0 Mbps.
    let mut wifi = interface("wlan0", 1);
    wifi.link_speed = ScalarObservation::unavailable(FailureKind::Unsupported);
    let inventory = ResolvedInventory {
        interfaces: vec![wifi],
        discovered_devices: vec![DeviceId::new("net:mac:aa:bb:cc:dd:ee:ff")],
        outcome: SourceOutcome::Available,
        fresh_count: 1,
        metadata_outcome: SourceOutcome::Available,
        metadata_item_count: 1,
        fresh_interfaces: HashSet::from([Arc::from("wlan0")]),
    };
    let counters = available_counters(HashMap::from([(
        Arc::from("wlan0"),
        CounterValues {
            rx_rate: ScalarObservation::available(10_000_000, 1_000),
            tx_rate: ScalarObservation::available(5_000_000, 1_000),
            utilization: ScalarObservation::unavailable(FailureKind::Unsupported),
            ..Default::default()
        },
    )]));
    // iw associated but with no tx bitrate parsed.
    let iw = SourceObservation {
        value: HashMap::from([(
            Arc::from("wlan0"),
            IwLinkResult::Associated {
                bssid: None,
                ssid: "studio".to_owned(),
                signal_dbm: Some(-50),
                frequency_mhz: None,
                channel: None,
                rx_bitrate_mbps: None,
                tx_bitrate_mbps: None,
                protocol: None,
            },
        )]),
        outcome: SourceOutcome::Available,
    };

    let snapshot = assemble_snapshot(
        inventory,
        counters,
        SourceObservation {
            value: HashMap::new(),
            outcome: SourceOutcome::Empty,
        },
        SourceObservation {
            value: HashMap::new(),
            outcome: SourceOutcome::Empty,
        },
        iw,
        1_000,
    );

    let metric = &snapshot.value[0];
    assert_eq!(
        metric.scalar_observations().link_speed_mbps.current_value(),
        None,
        "no bitrate to backfill from — link_speed stays without a current value"
    );
    assert_eq!(
        metric.scalar_observations().link_speed_mbps.availability(),
        taskmanager_core::ScalarAvailability::Unavailable(FailureKind::Unsupported),
        "the typed sysfs Unavailable state is preserved, not overwritten with a fabricated 0"
    );
    assert_eq!(
        metric.scalar_observations().utilization_pct.current_value(),
        None,
        "utilization stays unavailable when no link speed is known"
    );
}
