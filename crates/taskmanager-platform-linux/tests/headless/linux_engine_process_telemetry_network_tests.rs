use super::*;

const TCP: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/proc_net_tcp.txt"
));
const TCP6: &str = "  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n   0: 00000000000000000000000001000000:14E9 00000000000000000000000001000000:0035 01 00000000:00000000 00:00000000 00000000  1000        0 424244 1 0000000000000000 100 0 0 10 0\n";
const UNIX: &str = "Num       RefCount Protocol Flags    Type St Inode Path\n00000000: 00000002 00000000 00010000 0001 01 500 /run/task manager.sock\n00000000: 00000002 00000000 00000000 0002 03 501 @abstract-channel\n00000000: 00000002 00000000 00000000 0001 01 502\n";

#[test]
fn parses_ipv4_endpoints_state_and_inode() {
    let rows = parse_socket_table(TCP, ConnectionTransport::Tcp, ConnectionAddressFamily::Ipv4);
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0].local.as_socket_addr(),
        Some("127.0.0.1:8080".parse().unwrap())
    );
    assert_eq!(rows[0].family, ConnectionAddressFamily::Ipv4);
    assert_eq!(rows[0].state, ConnectionState::Listen);
    assert_eq!(
        rows[0]
            .provider_key
            .as_ref()
            .and_then(|key| key.as_numeric()),
        Some(424_242)
    );
    assert_eq!(
        rows[1].remote.as_socket_addr(),
        Some("10.0.0.2:443".parse().unwrap())
    );
    assert_eq!(rows[1].state, ConnectionState::Established);
}

#[test]
fn parses_ipv6_without_encoding_family_in_transport() {
    let rows = parse_socket_table(
        TCP6,
        ConnectionTransport::Tcp,
        ConnectionAddressFamily::Ipv6,
    );

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].transport, ConnectionTransport::Tcp);
    assert_eq!(rows[0].family, ConnectionAddressFamily::Ipv6);
    assert_eq!(rows[0].local.to_string(), "[::1]:5353");
    assert_eq!(rows[0].remote.to_string(), "[::1]:53");
}

#[test]
fn parses_named_abstract_and_unnamed_local_endpoints_without_dummy_ip() {
    let rows = parse_local_socket_table(UNIX);

    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows[0].local,
        ConnectionEndpoint::local("/run/task manager.sock")
    );
    assert_eq!(rows[0].remote, ConnectionEndpoint::Unspecified);
    assert_eq!(rows[0].state, ConnectionState::Listen);
    assert_eq!(
        rows[1].local,
        ConnectionEndpoint::local("@abstract-channel")
    );
    assert_eq!(rows[1].state, ConnectionState::Established);
    assert_eq!(rows[2].local, ConnectionEndpoint::Unspecified);
    assert!(
        rows.iter()
            .all(|row| !row.local.to_string().contains("0.0.0.0"))
    );
}

#[test]
fn socket_link_parser_is_strict() {
    assert_eq!(parse_socket_inode("socket:[123]"), Some(123));
    assert_eq!(parse_socket_inode("pipe:[123]"), None);
    assert_eq!(parse_socket_inode("socket:[bad]"), None);
}

#[cfg(target_os = "linux")]
#[test]
fn provider_filters_namespace_table_to_process_owned_inodes() {
    use std::os::unix::fs::symlink;

    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm-process-network-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("fd")).unwrap();
    std::fs::create_dir_all(root.join("net")).unwrap();
    symlink("socket:[424242]", root.join("fd/3")).unwrap();
    std::fs::write(root.join("net/tcp"), TCP).unwrap();
    std::fs::write(root.join("net/unix"), "header\n").unwrap();
    let snapshot = collect_from_proc_dir(&root, 100);
    assert_eq!(snapshot.connections.len(), 1);
    assert_eq!(
        snapshot.connections[0]
            .provider_key
            .as_ref()
            .and_then(|key| key.as_numeric()),
        Some(424_242)
    );
    assert_eq!(snapshot.rx_bytes_per_sec, None);
    std::fs::remove_dir_all(root).ok();
}

#[cfg(target_os = "linux")]
#[test]
fn provider_never_reports_healthy_when_socket_tables_are_unreadable() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm-process-network-state-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("fd")).unwrap();
    std::fs::create_dir_all(root.join("net")).unwrap();

    let unavailable = collect_from_proc_dir(&root, 100);
    assert_eq!(unavailable.state.status, DeviceStatus::Stale);
    assert_eq!(unavailable.state.last_success_ms, None);

    for table in ["tcp", "tcp6", "udp", "udp6", "unix"] {
        std::fs::write(root.join("net").join(table), "header\n").unwrap();
    }
    let recovered = collect_from_proc_dir(&root, 200);
    assert_eq!(recovered.state, DeviceState::healthy(200));
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn accounting_rates_need_two_samples_and_reset_on_pid_reuse_or_rollback() {
    let first_identity = ProcessIdentity {
        pid: 42,
        start_token: 10,
    };
    let mut tracker = ProcessNetworkRateTracker::default();
    let mut first = ProcessNetworkSnapshot::default();
    tracker.observe(
        first_identity,
        1_000,
        Some(ProviderId::borrowed("linux.ebpf.aya")),
        Ok(NetworkByteCounters {
            rx_bytes: 100,
            tx_bytes: 200,
        }),
        &mut first,
    );
    assert_eq!(first.rx_bytes_per_sec, None);
    assert_eq!(first.traffic_state, DeviceState::healthy(1_000));
    assert_eq!(first.traffic_failure, None);
    assert_eq!(
        first.traffic_provider.as_ref().map(ProviderId::as_str),
        Some("linux.ebpf.aya")
    );

    let mut second = ProcessNetworkSnapshot::default();
    tracker.observe(
        first_identity,
        2_000,
        Some(ProviderId::borrowed("linux.ebpf.aya")),
        Ok(NetworkByteCounters {
            rx_bytes: 4_100,
            tx_bytes: 2_200,
        }),
        &mut second,
    );
    assert_eq!(second.rx_bytes_per_sec, Some(4_000));
    assert_eq!(second.tx_bytes_per_sec, Some(2_000));

    let mut rollback = ProcessNetworkSnapshot::default();
    tracker.observe(
        first_identity,
        3_000,
        Some(ProviderId::borrowed("linux.ebpf.aya")),
        Ok(NetworkByteCounters {
            rx_bytes: 1,
            tx_bytes: 1,
        }),
        &mut rollback,
    );
    assert_eq!(rollback.rx_bytes_per_sec, None);
    assert_eq!(rollback.tx_bytes_per_sec, None);

    let mut reused = ProcessNetworkSnapshot::default();
    tracker.observe(
        ProcessIdentity {
            pid: 42,
            start_token: 11,
        },
        4_000,
        Some(ProviderId::borrowed("linux.ebpf.aya")),
        Ok(NetworkByteCounters {
            rx_bytes: 9_000,
            tx_bytes: 9_000,
        }),
        &mut reused,
    );
    assert_eq!(reused.rx_bytes_per_sec, None);
}

#[test]
fn accounting_failures_are_typed_and_preserve_last_success() {
    let identity = ProcessIdentity {
        pid: 7,
        start_token: 70,
    };
    let mut tracker = ProcessNetworkRateTracker::default();
    let mut healthy = ProcessNetworkSnapshot::default();
    tracker.observe(
        identity,
        100,
        Some(ProviderId::borrowed("linux.ebpf.aya")),
        Ok(NetworkByteCounters {
            rx_bytes: 1,
            tx_bytes: 2,
        }),
        &mut healthy,
    );
    let mut denied = ProcessNetworkSnapshot::default();
    tracker.observe(
        identity,
        200,
        Some(ProviderId::borrowed("linux.ebpf.aya")),
        Err(NetworkAccountingFailure::PermissionDenied),
        &mut denied,
    );
    assert_eq!(denied.traffic_state.status, DeviceStatus::PermissionDenied);
    assert_eq!(denied.traffic_state.last_success_ms, Some(100));
    assert_eq!(denied.rx_bytes_per_sec, None);
    assert_eq!(denied.traffic_failure, Some(FailureKind::PermissionDenied));
    assert_eq!(
        denied.traffic_provider.as_ref().map(ProviderId::as_str),
        Some("linux.ebpf.aya")
    );

    let mut recovered = ProcessNetworkSnapshot::default();
    tracker.observe(
        identity,
        300,
        Some(ProviderId::borrowed("linux.ebpf.aya")),
        Ok(NetworkByteCounters {
            rx_bytes: 3,
            tx_bytes: 4,
        }),
        &mut recovered,
    );
    assert_eq!(recovered.traffic_failure, None);
    assert_eq!(recovered.traffic_state, DeviceState::healthy(300));
}

/// The live-pid prune contract: entries for pids that left the authoritative
/// set are dropped (a re-observation re-seeds instead of rate-converting off
/// the dead baseline), while every live pid — including other open insight
/// targets — keeps its baseline untouched.
#[test]
fn rate_tracker_prunes_exited_pids_against_the_live_set() {
    fn observe_at(
        tracker: &mut ProcessNetworkRateTracker,
        identity: ProcessIdentity,
        now_ms: u64,
        rx_bytes: u64,
    ) -> ProcessNetworkSnapshot {
        let mut snapshot = ProcessNetworkSnapshot::default();
        tracker.observe(
            identity,
            now_ms,
            Some(ProviderId::borrowed("linux.afpacket")),
            Ok(NetworkByteCounters {
                rx_bytes,
                tx_bytes: rx_bytes,
            }),
            &mut snapshot,
        );
        snapshot
    }

    let live = ProcessIdentity {
        pid: 7,
        start_token: 70,
    };
    let exited = ProcessIdentity {
        pid: 8,
        start_token: 80,
    };
    let mut tracker = ProcessNetworkRateTracker::default();
    observe_at(&mut tracker, live, 1_000, 100);
    observe_at(&mut tracker, exited, 1_000, 100);

    tracker.retain_live_pids(&HashSet::from([7]));

    let kept = observe_at(&mut tracker, live, 2_000, 4_100);
    assert_eq!(kept.rx_bytes_per_sec, Some(4_000));
    let reseeded = observe_at(&mut tracker, exited, 2_000, 4_100);
    assert_eq!(
        reseeded.rx_bytes_per_sec, None,
        "a pruned pid must re-seed (first-sighting gap), not inherit its old baseline"
    );
    assert_eq!(reseeded.traffic_state, DeviceState::healthy(2_000));
}
