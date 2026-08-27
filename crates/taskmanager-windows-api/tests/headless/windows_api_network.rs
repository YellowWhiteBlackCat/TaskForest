use super::*;

#[test]
fn link_status_preserves_unknown_native_values() {
    assert_eq!(link_up_from_status(1), Some(true));
    assert_eq!(link_up_from_status(2), Some(false));
    assert_eq!(link_up_from_status(7), Some(false));
    assert_eq!(link_up_from_status(0), None);
    assert_eq!(link_up_from_status(99), None);
}

#[test]
fn zero_link_speed_is_not_reported_as_a_real_capacity() {
    assert_eq!(nonzero(0), None);
    assert_eq!(nonzero(1_000_000_000), Some(1_000_000_000));
}

#[cfg(windows)]
#[test]
fn live_network_adapters_query() {
    let result = enumerate_network_adapters();
    let adapters = result.expect("live network adapters query");
    eprintln!("FOUND {} ADAPTERS", adapters.len());
    for adapter in &adapters {
        eprintln!(
            "ADAPTER: name='{}', desc='{}', type={:?}, speed={:?}/{:?}, up={:?}",
            adapter.name,
            adapter.description,
            adapter.adapter_type,
            adapter.receive_link_speed_bps,
            adapter.transmit_link_speed_bps,
            adapter.link_up
        );
    }
    assert!(!adapters.is_empty());
}

#[cfg(not(windows))]
#[test]
fn adapter_query_is_typed_unsupported_off_windows() {
    assert_eq!(
        enumerate_network_adapters(),
        Err(WindowsApiError::Unsupported)
    );
}
