use super::*;

fn record(relationship: i32, payload_len: usize) -> Vec<u8> {
    let size = RECORD_HEADER_BYTES + payload_len;
    let mut bytes = vec![0_u8; size];
    bytes[..4].copy_from_slice(&relationship.to_le_bytes());
    bytes[4..8].copy_from_slice(&(size as u32).to_le_bytes());
    bytes
}

#[test]
fn cache_records_sum_each_windows_cache_instance() {
    let mut first = record(RELATION_CACHE, CACHE_FIXED_BYTES - RECORD_HEADER_BYTES);
    first[8] = 1;
    first[12..16].copy_from_slice(&(32 * 1024_u32).to_le_bytes());
    let mut second = record(RELATION_CACHE, CACHE_FIXED_BYTES - RECORD_HEADER_BYTES);
    second[8] = 3;
    second[12..16].copy_from_slice(&(16 * 1024_u32).to_le_bytes());
    let mut bytes = first;
    bytes.extend(second);
    assert_eq!(
        parse_cache_records(&bytes, RELATION_CACHE),
        Ok([Some(32), None, Some(16)])
    );
}

#[test]
fn malformed_variable_records_are_rejected_without_slicing() {
    let mut bytes = record(RELATION_CACHE, 0);
    bytes[4..8].copy_from_slice(&(u32::MAX).to_le_bytes());
    assert_eq!(
        parse_cache_records(&bytes, RELATION_CACHE),
        Err(WindowsApiError::QueryFailed)
    );
}

#[cfg(windows)]
#[test]
fn live_processor_topology_query() {
    let topo = processor_topology();
    assert!(
        topo.is_ok(),
        "processor topology must succeed on Windows host"
    );
    let topo = topo.unwrap();
    eprintln!("LIVE PROCESSOR TOPOLOGY: {topo:?}");
    assert!(topo.socket_count.is_some());
    assert!(topo.l1_cache_kb.is_some() || topo.l2_cache_kb.is_some() || topo.l3_cache_kb.is_some());
}
