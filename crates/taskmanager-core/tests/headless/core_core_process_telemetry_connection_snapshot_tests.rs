use super::*;
use std::net::SocketAddr;

#[test]
fn connection_scope_distinguishes_pure_loopback_from_external_peer() {
    let loopback = ProcessConnection {
        transport: ConnectionTransport::Tcp,
        family: ConnectionAddressFamily::Ipv4,
        local: "127.0.0.1:1000".parse::<SocketAddr>().unwrap().into(),
        remote: "127.0.0.1:2000".parse::<SocketAddr>().unwrap().into(),
        state: ConnectionState::Established,
        provider_key: None,
        rtt_ms: None,
    };
    assert!(loopback.is_loopback());
    assert!(ConnectionEndpoint::local("/run/service.sock").is_loopback());

    let external = ProcessConnection {
        local: "127.0.0.1:1000".parse::<SocketAddr>().unwrap().into(),
        remote: "192.0.2.1:443".parse::<SocketAddr>().unwrap().into(),
        ..loopback.clone()
    };
    assert!(!external.is_loopback());
}

#[test]
fn traffic_provenance_roundtrips_and_old_snapshots_default_safely() {
    let snapshot = ProcessNetworkSnapshot {
        traffic_failure: Some(FailureKind::Rejected),
        traffic_provider: Some(ProviderId::borrowed("linux.ebpf.aya")),
        ..Default::default()
    };
    let json = serde_json::to_string(&snapshot).expect("serialize network snapshot");
    let decoded: ProcessNetworkSnapshot =
        serde_json::from_str(&json).expect("deserialize network snapshot");
    assert_eq!(decoded.traffic_failure, Some(FailureKind::Rejected));
    assert_eq!(
        decoded.traffic_provider.as_ref().map(ProviderId::as_str),
        Some("linux.ebpf.aya")
    );

    let mut old_json =
        serde_json::to_value(ProcessNetworkSnapshot::default()).expect("serialize old fixture");
    let object = old_json
        .as_object_mut()
        .expect("network snapshot serializes as an object");
    object.remove("traffic_failure");
    object.remove("traffic_provider");
    let old: ProcessNetworkSnapshot =
        serde_json::from_value(old_json).expect("deserialize pre-provenance snapshot");
    assert_eq!(old.traffic_failure, None);
    assert_eq!(old.traffic_provider, None);
}

#[test]
fn connection_state_covers_standard_tcp_states_and_serde_roundtrips() {
    let cases = [
        (ConnectionState::Established, "established", "ESTABLISHED"),
        (ConnectionState::SynSent, "syn_sent", "SYN_SENT"),
        (ConnectionState::SynReceived, "syn_received", "SYN_RECEIVED"),
        (ConnectionState::FinWait1, "fin_wait1", "FIN_WAIT_1"),
        (ConnectionState::FinWait2, "fin_wait2", "FIN_WAIT_2"),
        (ConnectionState::TimeWait, "time_wait", "TIME_WAIT"),
        (ConnectionState::Closed, "closed", "CLOSED"),
        (ConnectionState::CloseWait, "close_wait", "CLOSE_WAIT"),
        (ConnectionState::LastAck, "last_ack", "LAST_ACK"),
        (ConnectionState::Listen, "listen", "LISTEN"),
        (ConnectionState::Closing, "closing", "CLOSING"),
        (ConnectionState::Unconnected, "unconnected", "UNCONNECTED"),
        (ConnectionState::Unknown, "unknown", "UNKNOWN"),
    ];

    for (state, snake_str, display_str) in cases {
        let serialized = serde_json::to_string(&state).expect("serialize connection state");
        assert_eq!(serialized, format!("\"{snake_str}\""));

        let deserialized: ConnectionState =
            serde_json::from_str(&serialized).expect("deserialize connection state");
        assert_eq!(deserialized, state);

        assert_eq!(state.as_str(), display_str);
        assert_eq!(format!("{state}"), display_str);
    }
}

#[test]
fn connection_state_classification_predicates_and_defaults() {
    assert_eq!(ConnectionState::default(), ConnectionState::Unknown);

    assert!(ConnectionState::Established.is_established());
    assert!(!ConnectionState::SynSent.is_established());
    assert!(!ConnectionState::Listen.is_established());

    assert!(ConnectionState::Listen.is_listening());
    assert!(!ConnectionState::Established.is_listening());
    assert!(!ConnectionState::TimeWait.is_listening());

    assert!(ConnectionState::CloseWait.is_closing());
    assert!(ConnectionState::TimeWait.is_closing());
    assert!(ConnectionState::FinWait1.is_closing());
    assert!(ConnectionState::FinWait2.is_closing());
    assert!(ConnectionState::LastAck.is_closing());
    assert!(ConnectionState::Closing.is_closing());
    assert!(!ConnectionState::Established.is_closing());
    assert!(!ConnectionState::Listen.is_closing());
    assert!(!ConnectionState::SynSent.is_closing());
    assert!(!ConnectionState::Closed.is_closing());
}

#[test]
fn connection_state_hash_and_equality() {
    use std::collections::HashSet;

    let mut set = HashSet::new();
    set.insert(ConnectionState::Established);
    set.insert(ConnectionState::Listen);
    set.insert(ConnectionState::TimeWait);
    set.insert(ConnectionState::CloseWait);
    set.insert(ConnectionState::SynSent);

    assert!(set.contains(&ConnectionState::Established));
    assert!(set.contains(&ConnectionState::Listen));
    assert!(set.contains(&ConnectionState::TimeWait));
    assert!(set.contains(&ConnectionState::CloseWait));
    assert!(set.contains(&ConnectionState::SynSent));
    assert!(!set.contains(&ConnectionState::Unknown));
    assert!(!set.contains(&ConnectionState::Closed));
}
