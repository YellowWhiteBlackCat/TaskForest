use super::{StartupImpact, StartupSource};

#[test]
fn impact_thresholds_map_millis_to_the_typed_band() {
    // Boundary values around both thresholds: 500/501 and 100/101.
    for (millis, expected) in [
        (0, StartupImpact::Low),
        (100, StartupImpact::Low),
        (101, StartupImpact::Medium),
        (500, StartupImpact::Medium),
        (501, StartupImpact::High),
        (u64::MAX, StartupImpact::High),
    ] {
        assert_eq!(
            StartupImpact::from_millis(millis),
            expected,
            "impact for {millis}ms"
        );
    }
}

#[test]
fn impact_i18n_keys_are_non_empty_and_unique_per_variant() {
    let mut keys = std::collections::HashSet::new();
    for variant in [
        StartupImpact::High,
        StartupImpact::Medium,
        StartupImpact::Low,
        StartupImpact::None,
    ] {
        let key = variant.i18n_key();
        assert!(!key.is_empty(), "{variant:?} must have a catalog key");
        assert!(keys.insert(key), "catalog key {key:?} must be unique");
    }
}

#[test]
fn source_labels_are_non_empty_and_unique_per_variant() {
    let mut labels = std::collections::HashSet::new();
    for variant in [
        StartupSource::DesktopEntry,
        StartupSource::UserService,
        StartupSource::SystemService,
        StartupSource::RunLevel,
        StartupSource::RegistryEntry,
        StartupSource::ScheduledTask,
        StartupSource::LoginItem,
        StartupSource::StartupFolder,
        StartupSource::Other,
    ] {
        let label = variant.as_str();
        assert!(!label.is_empty(), "{variant:?} must have a label");
        assert!(labels.insert(label), "label {label:?} must be unique");
    }
}
