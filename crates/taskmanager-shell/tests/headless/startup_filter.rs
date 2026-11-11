use super::*;

#[test]
fn startup_status_filter_keeps_only_the_selected_enabled_state() {
    let cases = [
        (true, StartupStatusFilter::All, true),
        (false, StartupStatusFilter::All, true),
        (true, StartupStatusFilter::Enabled, true),
        (false, StartupStatusFilter::Enabled, false),
        (true, StartupStatusFilter::Disabled, false),
        (false, StartupStatusFilter::Disabled, true),
    ];

    for (enabled, filter, expected) in cases {
        assert_eq!(
            matches_startup_status(enabled, filter),
            expected,
            "enabled={enabled}, filter={filter:?}"
        );
    }
}

#[test]
fn startup_status_catalog_is_complete_localized_and_keyed() {
    let keys: Vec<_> = StartupStatusFilter::ALL
        .into_iter()
        .map(StartupStatusFilter::key)
        .collect();

    assert_eq!(keys, ["all", "enabled", "disabled"]);
    assert_eq!(StartupStatusFilter::default(), StartupStatusFilter::All);
    assert!(
        StartupStatusFilter::ALL
            .into_iter()
            .all(|filter| { !filter.label().is_empty() })
    );
}
