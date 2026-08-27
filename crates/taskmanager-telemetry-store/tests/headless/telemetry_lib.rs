use super::*;

#[test]
fn correlated_histories_start_empty_instead_of_fabricating_zeroes() {
    let (store, _ingestor) = TelemetryStore::shared_with_correlated_ingestion(2);

    assert!(store.system_history.cpu_usage().samples().is_empty());
    assert!(store.system_history.memory_usage().samples().is_empty());
    assert!(store.system_history.swap_usage().samples().is_empty());
}
