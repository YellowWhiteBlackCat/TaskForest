use std::net::SocketAddr;

use crate::core::DeviceState;
use crate::core::identity::ProviderId;

use super::*;

#[test]
fn legacy_ipv4_connection_json_round_trips_without_wire_shape_changes() {
    let legacy = serde_json::json!({
        "protocol": "tcp",
        "local": "127.0.0.1:8080",
        "remote": "10.0.0.2:443",
        "state": "established",
        "provider_key": 42
    });

    let decoded: ProcessConnection =
        serde_json::from_value(legacy.clone()).expect("legacy IPv4 connection should decode");
    assert_eq!(decoded.transport, ConnectionTransport::Tcp);
    assert_eq!(decoded.family, ConnectionAddressFamily::Ipv4);
    assert_eq!(decoded.local.to_string(), "127.0.0.1:8080");

    let encoded =
        serde_json::to_value(decoded).expect("IPv4 connection should serialize compatibly");
    assert_eq!(encoded, legacy);
}

#[test]
fn legacy_ipv6_protocol_alias_splits_transport_from_family() {
    let legacy = serde_json::json!({
        "protocol": "udp6",
        "local": "[::1]:5353",
        "remote": "[::1]:53",
        "state": "unconnected",
        "provider_key": 43
    });

    let decoded: ProcessConnection =
        serde_json::from_value(legacy.clone()).expect("legacy IPv6 connection should decode");
    assert_eq!(decoded.transport, ConnectionTransport::Udp);
    assert_eq!(decoded.family, ConnectionAddressFamily::Ipv6);
    assert_eq!(serde_json::to_value(decoded).unwrap(), legacy);
}

#[test]
fn all_legacy_inet_protocol_names_remain_wire_compatible() {
    for (protocol, local, remote, transport, family) in [
        (
            "tcp",
            "127.0.0.1:80",
            "10.0.0.1:443",
            ConnectionTransport::Tcp,
            ConnectionAddressFamily::Ipv4,
        ),
        (
            "tcp6",
            "[::1]:80",
            "[2001:db8::1]:443",
            ConnectionTransport::Tcp,
            ConnectionAddressFamily::Ipv6,
        ),
        (
            "udp",
            "127.0.0.1:53",
            "10.0.0.1:53",
            ConnectionTransport::Udp,
            ConnectionAddressFamily::Ipv4,
        ),
        (
            "udp6",
            "[::1]:53",
            "[2001:db8::1]:53",
            ConnectionTransport::Udp,
            ConnectionAddressFamily::Ipv6,
        ),
    ] {
        let legacy = serde_json::json!({
            "protocol": protocol,
            "local": local,
            "remote": remote,
            "state": "unconnected",
            "provider_key": 99
        });
        let decoded: ProcessConnection =
            serde_json::from_value(legacy.clone()).expect("legacy INET connection");
        assert_eq!(decoded.transport, transport);
        assert_eq!(decoded.family, family);
        assert_eq!(serde_json::to_value(decoded).unwrap(), legacy);
    }
}

#[test]
fn local_and_opaque_endpoints_never_require_dummy_ip_addresses() {
    let local = ProcessConnection {
        transport: ConnectionTransport::Local,
        family: ConnectionAddressFamily::Local,
        local: ConnectionEndpoint::local("/run/taskmanager.sock"),
        remote: ConnectionEndpoint::Unspecified,
        state: ConnectionState::Listen,
        provider_key: Some(44.into()),
    };
    let local_json = serde_json::to_value(&local).expect("local connection should serialize");
    assert_eq!(local_json["protocol"], "local");
    assert_eq!(local_json["family"], "local");
    assert_eq!(
        local_json["local"],
        serde_json::json!({"kind": "local", "path": "/run/taskmanager.sock"})
    );
    assert_eq!(
        local_json["remote"],
        serde_json::json!({"kind": "unspecified"})
    );
    assert!(!local_json.to_string().contains("0.0.0.0"));
    assert_eq!(
        serde_json::from_value::<ProcessConnection>(local_json).unwrap(),
        local
    );

    let opaque = ProcessConnection {
        transport: ConnectionTransport::Other("native-stream".into()),
        family: ConnectionAddressFamily::Other("native-family".into()),
        local: ConnectionEndpoint::opaque("namespace:channel-7"),
        remote: ConnectionEndpoint::Unspecified,
        state: ConnectionState::Unknown,
        provider_key: Some(ConnectionProviderKey::Composite(vec![
            "namespace".into(),
            "channel-7".into(),
        ])),
    };
    let opaque_json = serde_json::to_value(&opaque).expect("opaque connection should serialize");
    assert_eq!(opaque_json["protocol"], "native-stream");
    assert_eq!(opaque_json["family"], "native-family");
    assert_eq!(
        opaque_json["provider_key"],
        serde_json::json!({"parts": ["namespace", "channel-7"]})
    );
    assert_eq!(
        serde_json::from_value::<ProcessConnection>(opaque_json).unwrap(),
        opaque
    );
}

#[test]
fn socket_address_conversion_and_display_preserve_ip_endpoint() {
    let address: SocketAddr = "[2001:db8::1]:443".parse().expect("fixture socket");
    let endpoint = ConnectionEndpoint::from(address);

    assert_eq!(endpoint.as_socket_addr(), Some(address));
    assert_eq!(endpoint.to_string(), "[2001:db8::1]:443");
}

#[test]
fn neutral_resource_group_api_preserves_legacy_snapshot_keys() {
    let groups = vec![ResourceGroupMembership {
        provider: ProviderId::borrowed("fixture.job-object"),
        native_hierarchy_id: Some(7),
        capabilities: vec!["memory".into(), "cpu".into()],
        native_locator: "job:fixture".into(),
    }];
    let snapshot = ProcessResourceSnapshot::from_observations(
        DeviceState::healthy(10),
        ProcessResourceObservations {
            resource_groups: ResourceObservation::current(groups, 10),
            memory_usage_bytes: ResourceObservation::current(64, 10),
            memory_limit: ResourceObservation::current(LimitValue::Value(128), 10),
            cpu_time_quota_micros: ResourceObservation::current(LimitValue::Value(20), 10),
            cpu_time_period_micros: ResourceObservation::current(100, 10),
            process_count: ResourceObservation::current(2, 10),
            process_limit: ResourceObservation::current(LimitValue::Value(4), 10),
            ..ProcessResourceObservations::default()
        },
        Vec::new(),
    );

    let value = serde_json::to_value(&snapshot).expect("resource snapshot should serialize");

    assert_eq!(value["groups"][0]["provider"], "fixture.job-object");
    assert_eq!(value["groups"][0]["path"], "job:fixture");
    assert_eq!(value["memory_current_bytes"], 64);
    assert_eq!(value["cpu_quota_us"]["value"], 20);
    assert_eq!(value["pids_current"], 2);
    assert!(value.get("resource_groups").is_none());

    let decoded: ProcessResourceSnapshot =
        serde_json::from_value(value).expect("legacy resource keys should decode");
    assert_eq!(
        decoded.current_resource_groups().unwrap()[0]
            .provider
            .as_str(),
        "fixture.job-object"
    );
    assert_eq!(
        decoded.current_resource_groups().unwrap()[0].capabilities,
        ["memory", "cpu"]
    );
    assert_eq!(decoded.current_memory_usage_bytes(), Some(64));
    assert_eq!(
        decoded.current_cpu_time_quota_micros(),
        Some(LimitValue::Value(20))
    );
    assert_eq!(decoded.current_process_count(), Some(2));
}
