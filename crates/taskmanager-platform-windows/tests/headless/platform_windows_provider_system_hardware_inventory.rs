use super::*;

#[test]
fn package_count_source_rides_the_hardware_inventory_snapshot() {
    let mut provider = WinHardwareInventoryProvider::new();
    let snapshot = provider
        .refresh()
        .expect("hardware inventory refresh composes headlessly");
    let package_source = snapshot
        .sources
        .iter()
        .find(|status| status.provider == PACKAGE_COUNT_PROVIDER)
        .expect("package-count source is always accounted for");
    #[cfg(windows)]
    {
        // The per-machine ARP hive exists on every Windows install, so the
        // count is real and the outcome never lies about it.
        eprintln!(
            "LIVE PACKAGE COUNT: {:?} ({:?})",
            snapshot.value.package_count, package_source.outcome
        );
        assert!(snapshot.value.package_count.is_some());
        assert_ne!(package_source.outcome, SourceOutcome::Empty);
        assert!(!matches!(
            package_source.outcome,
            SourceOutcome::Unavailable(_)
        ));
    }
    #[cfg(not(windows))]
    {
        // Off-Windows the registry-backed source is absent: the count stays
        // the never-observed None with an explicit MissingDependency
        // receipt, never a fabricated zero.
        assert!(snapshot.value.package_count.is_none());
        assert_eq!(
            package_source.outcome,
            SourceOutcome::Unavailable(FailureKind::MissingDependency)
        );
        assert_eq!(package_source.item_count, 0);
    }
}

#[test]
fn package_facts_carry_no_package_manager_attribution() {
    // A Windows ARP count is not backed by any one native package database,
    // so the manager fields stay absent even when the count exists.
    let mut provider = WinHardwareInventoryProvider::new();
    let snapshot = provider
        .refresh()
        .expect("hardware inventory refresh composes headlessly");
    assert!(snapshot.value.package_manager.is_none());
    assert!(snapshot.value.package_manager_version.is_none());
}
