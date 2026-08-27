use super::*;
use std::net::{IpAddr, Ipv4Addr};
use taskmanager_afpacket::FiveTuple;

fn build(owners: &[(u64, u32)], flows: &[(u8, &str, u16, &str, u16, u64)]) -> PacketAttribution {
    let mut a = PacketAttribution::default();
    for (inode, pid) in owners {
        a.owners.insert(*inode, *pid);
    }
    for (proto, lip, lport, rip, rport, inode) in flows {
        let local = SocketAddr::new(lip.parse().unwrap(), *lport);
        let remote = SocketAddr::new(rip.parse().unwrap(), *rport);
        let (lo, hi) = sort_pair(local, remote);
        a.flows.insert((*proto, lo, hi), *inode);
    }
    a
}

fn tuple(src: &str, sp: u16, dst: &str, dp: u16, proto: u8) -> FiveTuple {
    FiveTuple {
        src: src.parse().unwrap(),
        dst: dst.parse().unwrap(),
        proto,
        src_port: sp,
        dst_port: dp,
    }
}

#[test]
fn attributes_established_flow_in_either_direction() {
    // pid 4321 owns inode 99; inode 99 is the 10.0.0.1:443 ↔ 192.168.1.5:54321 flow.
    let attribution = build(
        &[(99, 4321)],
        &[(TCP, "10.0.0.1", 443, "192.168.1.5", 54321, 99)],
    );
    // Outbound (src is the local 192.168.1.5).
    let outbound = tuple("192.168.1.5", 54321, "10.0.0.1", 443, TCP);
    assert_eq!(attribution.attribute(&outbound), Some(4321));
    // Inbound orientation of the same flow must attribute identically.
    let inbound = tuple("10.0.0.1", 443, "192.168.1.5", 54321, TCP);
    assert_eq!(attribution.attribute(&inbound), Some(4321));
}

#[test]
fn returns_none_for_unknown_flow_or_missing_owner() {
    let attribution = build(&[(7, 100)], &[(UDP, "8.8.8.8", 53, "1.1.1.1", 41000, 7)]);
    // Unknown endpoint pair.
    assert_eq!(
        attribution.attribute(&tuple("9.9.9.9", 99, "1.1.1.1", 41000, UDP)),
        None
    );
    // Known flow but inode has no owner (pid exited) — inode 88 absent.
    let orphan = build(&[], &[(UDP, "8.8.8.8", 53, "1.1.1.1", 41000, 88)]);
    assert_eq!(
        orphan.attribute(&tuple("1.1.1.1", 41000, "8.8.8.8", 53, UDP)),
        None
    );
}

#[test]
fn ignores_non_tcp_udp_protocols() {
    let attribution = build(&[(1, 1)], &[(TCP, "1.1.1.1", 1, "2.2.2.2", 2, 1)]);
    // ICMP (proto 1) is never indexed even if endpoints coincide.
    assert_eq!(
        attribution.attribute(&tuple("1.1.1.1", 1, "2.2.2.2", 2, 1)),
        None
    );
}

#[test]
fn from_proc_root_joins_fd_owner_to_socket_table() {
    // A minimal /proc tree: pid 1337 holds fd 3 → socket:[42]; the host TCP
    // table lists the 10.0.0.1:80 ↔ 172.16.0.9:5000 established flow on inode 42.
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm-net-accounting-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let fd_dir = root.join("1337/fd");
    std::fs::create_dir_all(&fd_dir).unwrap();
    std::fs::write(fd_dir.join("0"), b"/dev/null").unwrap(); // non-socket fd, ignored
    std::os::unix::fs::symlink("socket:[42]", fd_dir.join("3")).unwrap();
    std::fs::create_dir_all(root.join("net")).unwrap();
    // /proc/net/tcp header + one established row; inode is column 10, local
    // column 2, remote column 3 (hex IP:port, little-endian).
    std::fs::write(
            root.join("net/tcp"),
            b"  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
              0: 0100000A:0050 090010AC:C350 01 00000000:00000000 00:00000000 00000000     0        0 42 1 0000000000000000 100 0 0 10 -1\n",
        ).unwrap();
    // Non-numeric /proc entries (e.g. "self") are skipped, not fatal.
    std::fs::create_dir_all(root.join("self")).unwrap();

    let attribution = PacketAttribution::from_proc_root(&root);
    // Outbound from the local 172.16.0.9:50000 (0xC350 = 50000) to 10.0.0.1:80.
    let pkt = FiveTuple {
        src: IpAddr::V4(Ipv4Addr::new(172, 16, 0, 9)),
        src_port: 50000,
        dst: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
        dst_port: 80,
        proto: TCP,
    };
    assert_eq!(attribution.attribute(&pkt), Some(1337));
}

#[test]
fn afpacket_backend_degrades_to_requires_escalation_without_cap() {
    // On unprivileged CI the AF_PACKET open fails with EPERM; the gate
    // confirms PerProcessNet is escalatable, so the backend must degrade to
    // RequiresEscalation (the honest "offer the prompt" signal) — not a bare
    // PermissionDenied and never fabricated zero bytes.
    let mut backend = AfPacketAccountingBackend::start(Path::new("/proc"), 1);
    let identity = ProcessIdentity {
        pid: 1,
        start_token: 0,
    };
    let result = backend.read_counters(identity, 0);
    // If the test host actually holds CAP_NET_RAW (root / a setcap'd runner),
    // the open SUCCEEDS → the backend is Capturing, returning Ok(zero counters)
    // before any packet arrives. That environment is not this test's concern;
    // skip rather than assert the degrade path.
    if result.is_ok() {
        eprintln!("skipped: test host has CAP_NET_RAW (open succeeded), degrade path N/A");
        return;
    }
    assert_eq!(result, Err(NetworkAccountingFailure::RequiresEscalation));
}

/// `classify_afpacket_io` table: a permission denial maps to the
/// escalatable variant (the gate confirms `PerProcessNet` is escalatable
/// under the unprivileged default — offer the prompt), and every other
/// open failure (interface gone, resource limits) is `Unavailable`.
#[test]
fn classify_afpacket_io_maps_only_permission_denials_to_escalatable() {
    for (kind, expected) in [
        (
            io::ErrorKind::PermissionDenied,
            NetworkAccountingFailure::RequiresEscalation,
        ),
        (
            io::ErrorKind::NotFound,
            NetworkAccountingFailure::Unavailable,
        ),
        (
            io::ErrorKind::InvalidInput,
            NetworkAccountingFailure::Unavailable,
        ),
    ] {
        let error = io::Error::new(kind, "fixture open failure");
        assert_eq!(
            classify_afpacket_io(&error),
            expected,
            "{kind:?} must classify deterministically without touching host state",
        );
    }
}

/// Build a minimal Ethernet+IPv4+TCP frame for a given 5-tuple (mirrors the
/// boundary-crate parser's own fixture builder).
fn tcp_frame(src_ip: [u8; 4], src_port: u16, dst_ip: [u8; 4], dst_port: u16) -> Vec<u8> {
    let mut f = vec![0u8; 14 + 20 + 20];
    f[12] = 0x08; // ethertype IPv4
    f[13] = 0x00;
    f[14] = 0x45; // IPv4 version 4, IHL 5
    f[23] = 6; // proto TCP
    f[26..30].copy_from_slice(&src_ip);
    f[30..34].copy_from_slice(&dst_ip);
    f[34..36].copy_from_slice(&src_port.to_be_bytes());
    f[36..38].copy_from_slice(&dst_port.to_be_bytes());
    f
}

#[test]
fn accumulate_attributes_a_frame_and_splits_rx_from_tx() {
    // pid 7 owns inode 5; inode 5 is the 10.0.0.1:443 ↔ 192.168.1.5:5000 flow.
    let attribution = build(&[(5, 7)], &[(TCP, "10.0.0.1", 443, "192.168.1.5", 5000, 5)]);
    let counters: CounterMap = Arc::new(Mutex::new(HashMap::new()));
    // A received (incoming) frame on that flow → rx.
    let rx_frame = tcp_frame([10, 0, 0, 1], 443, [192, 168, 1, 5], 5000);
    accumulate(
        &attribution,
        &counters,
        CapturedPacket {
            frame: &rx_frame,
            outgoing: false,
        },
    );
    // A sent (outgoing) frame → tx.
    let tx_frame = tcp_frame([192, 168, 1, 5], 5000, [10, 0, 0, 1], 443);
    accumulate(
        &attribution,
        &counters,
        CapturedPacket {
            frame: &tx_frame,
            outgoing: true,
        },
    );
    let map = counters.lock().unwrap();
    let pid7 = map
        .get(&7)
        .expect("pid 7 attributed from the established flow");
    assert!(pid7.rx_bytes > 0, "incoming frame charged to rx");
    assert!(pid7.tx_bytes > 0, "outgoing frame charged to tx");
}

#[test]
fn default_route_iface_reads_proc_net_route() {
    // A /proc/net/route fixture whose default route (dest 00000000) is eth0.
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm-afpacket-route-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("sys/class/net/eth0")).unwrap();
    std::fs::write(root.join("sys/class/net/eth0/ifindex"), b"2\n").unwrap();
    std::fs::create_dir_all(root.join("net")).unwrap();
    std::fs::write(
            root.join("net/route"),
            b"Iface\tDestination\tGateway\tFlags\nlo\t00000000\t0100007F\t0003\neth0\t00000000\t0100A8C0\t0003\n",
        )
        .unwrap();
    assert_eq!(default_route_iface_index(&root), 2);
    std::fs::remove_dir_all(&root).ok();
}

/// The attribution-rebuild prune: per-pid counters survive only while the pid
/// is in the freshly enumerated live set, and a failed enumeration (empty
/// live set) prunes nothing rather than wiping counters for live processes.
#[test]
fn rebuild_prune_drops_counters_only_for_exited_pids() {
    let counters: CounterMap = Arc::new(Mutex::new(HashMap::from([
        (
            1337_u32,
            NetworkByteCounters {
                rx_bytes: 10,
                tx_bytes: 20,
            },
        ),
        (
            999_u32,
            NetworkByteCounters {
                rx_bytes: 1,
                tx_bytes: 2,
            },
        ),
    ])));
    // A minimal /proc fixture: pid 1337 is still running, 999 has exited
    // (no directory), and a non-numeric entry must not count as a pid.
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm-net-accounting-prune-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("1337")).unwrap();
    std::fs::create_dir_all(root.join("self")).unwrap();
    let attribution = PacketAttribution::from_proc_root(&root);
    assert_eq!(
        attribution.live_pids,
        HashSet::from([1337]),
        "the live set is the numeric pid population, not the socket owners"
    );

    retain_live_counters(&counters, &attribution.live_pids);
    {
        let map = counters.lock().unwrap();
        assert!(map.contains_key(&1337), "live pid keeps its counters");
        assert!(!map.contains_key(&999), "exited pid's entry is dropped");
    }

    // Fail-closed: an empty live set (the /proc walk failed) keeps everything.
    retain_live_counters(&counters, &HashSet::new());
    assert!(
        counters.lock().unwrap().contains_key(&1337),
        "a failed enumeration must not prune live pids' counters"
    );
    std::fs::remove_dir_all(&root).ok();
}
