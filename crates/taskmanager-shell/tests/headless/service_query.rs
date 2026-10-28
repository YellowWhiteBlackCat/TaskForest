use super::*;
use taskmanager_core::core::services::{ServiceItem, ServiceStatus};

fn service(name: &str, description: &str) -> ServiceItem {
    ServiceItem::from_inventory(name, name, ServiceStatus::Active, description, "", "", "")
}

#[test]
fn service_query_matches_name_and_description_with_ascii_case_folding() {
    let network = service("NetworkManager", "Network connection manager");
    assert!(matches_service_query(&network, ""));
    assert!(matches_service_query(&network, "   "));
    assert!(matches_service_query(&network, "network"));
    assert!(matches_service_query(&network, "  NETWORK  "));
    assert!(matches_service_query(&network, "connection"));
    assert!(!matches_service_query(&network, "docker"));
}

#[test]
fn service_query_is_shared_by_name_only_rows() {
    let nameless_description = service("dbus", "Message bus");
    assert!(matches_service_query(&nameless_description, "dbus"));
    assert!(matches_service_query(&nameless_description, "bus"));
    assert!(!matches_service_query(&nameless_description, "sshd"));
}
