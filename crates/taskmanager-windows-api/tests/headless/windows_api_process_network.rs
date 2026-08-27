use super::*;

#[test]
fn live_process_network_connections_query() {
    let result = query_process_network_connections();
    #[cfg(windows)]
    {
        let connections = result.expect("query process network connections");
        eprintln!("FOUND {} LIVE PROCESS CONNECTIONS", connections.len());
        if let Some(conn) = connections.first() {
            eprintln!("SAMPLE CONNECTION: {conn:?}");
        }
        assert!(!connections.is_empty());
    }
    #[cfg(not(windows))]
    {
        assert_eq!(result, Err(WindowsApiError::Unsupported));
    }
}
