use super::*;

/// The ten categories are exhaustive, ordered, and each names one catalog
/// entry. A renderer that iterates `ALL` reaches every persisted switch.
#[test]
fn the_category_set_is_exhaustive_and_ordered() {
    assert_eq!(DeviceVisibilityKey::ALL.len(), 10);
    let mut seen: Vec<&'static str> = Vec::new();
    for key in DeviceVisibilityKey::ALL {
        let i18n = key.i18n_key();
        assert!(
            i18n.starts_with("settings.show_"),
            "{key:?} has a foreign key"
        );
        assert!(
            !seen.contains(&i18n),
            "category key {i18n:?} is shared by two categories"
        );
        seen.push(i18n);
    }
    assert_eq!(seen.len(), 10);
}

/// The stable widget id is unique per category and is not a locale key, so
/// a focus target never depends on the active language.
#[test]
fn the_stable_widget_ids_are_unique_and_language_free() {
    let mut seen: Vec<&'static str> = Vec::new();
    for key in DeviceVisibilityKey::ALL {
        let id = key.stable_id();
        assert!(
            !id.contains('.'),
            "{key:?} id must not look like a locale key"
        );
        assert!(
            !seen.contains(&id),
            "widget id {id:?} is shared by two categories"
        );
        seen.push(id);
    }
    assert_eq!(seen.len(), 10);
}

/// The config fold round-trips every switch, and a toggle of one category
/// never disturbs its neighbours.
#[test]
fn the_config_fold_round_trips_every_switch() {
    let mut config = Config::default();
    let mut preferences = DeviceVisibilityPreferences::from_config(&config);
    assert!(preferences.visible(DeviceVisibilityKey::Cpu));

    for key in DeviceVisibilityKey::ALL {
        let mut next = preferences;
        let current = next.visible(key);
        let flipped = next.set_visible(key, !current);
        assert!(flipped, "{key:?} must report a real change");
        assert_eq!(
            next.visible(key),
            !current,
            "{key:?} must hold the new value"
        );
        assert!(!next.set_visible(key, !current), "{key:?} no-op is inert");
        preferences = next;
    }

    preferences.apply_to(&mut config);
    let read_back = DeviceVisibilityPreferences::from_config(&config);
    assert_eq!(read_back, preferences);
    for key in DeviceVisibilityKey::ALL {
        assert_eq!(
            read_back.visible(key),
            preferences.visible(key),
            "{key:?} must survive the config round-trip"
        );
    }
}

/// The network sub-categories are exactly the five Mission Center
/// interface classes, and the master network toggle is not one of them.
#[test]
fn the_network_subcategories_exclude_the_master_toggle() {
    assert_eq!(DeviceVisibilityKey::NETWORK_SUBCATEGORIES.len(), 5);
    for key in DeviceVisibilityKey::NETWORK_SUBCATEGORIES {
        assert!(
            key.is_network_subcategory(),
            "{key:?} must be a sub-category"
        );
        assert_ne!(key, DeviceVisibilityKey::Network);
    }
    assert!(!DeviceVisibilityKey::Network.is_network_subcategory());
    assert!(!DeviceVisibilityKey::Cpu.is_network_subcategory());
}
