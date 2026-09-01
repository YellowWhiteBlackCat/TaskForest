use super::*;

fn changed_axes(before: PresentationFingerprint, after: PresentationFingerprint) -> [bool; 6] {
    [
        before.appearance != after.appearance,
        before.devices != after.devices,
        before.units != after.units,
        before.graphs != after.graphs,
        before.sidebar != after.sidebar,
        before.apps != after.apps,
    ]
}

#[test]
fn named_mutations_invalidate_only_their_owned_projection_axis() {
    let mut preferences = PresentationPreferences::default();

    let before = preferences.fingerprint();
    preferences.set_device_visible(DevicePreference::Cpu, false);
    assert_eq!(
        changed_axes(before, preferences.fingerprint()),
        [false, true, false, false, false, false]
    );

    let before = preferences.fingerprint();
    preferences.set_quantity_notation(UnitFamily::Network, QuantityNotation::Bytes);
    assert_eq!(
        changed_axes(before, preferences.fingerprint()),
        [false, false, true, false, false, false]
    );

    let before = preferences.fingerprint();
    preferences.set_graphs(GraphPreferences {
        data_points: 240,
        ..preferences.graphs()
    });
    assert_eq!(
        changed_axes(before, preferences.fingerprint()),
        [false, false, false, true, false, false]
    );

    let before = preferences.fingerprint();
    preferences.set_sidebar(SidebarPreferences {
        width: gpui::px(320.0),
        ..preferences.sidebar().clone()
    });
    assert_eq!(
        changed_axes(before, preferences.fingerprint()),
        [false, false, false, false, true, false]
    );

    let before = preferences.fingerprint();
    preferences.set_gray_zero_values(true);
    assert_eq!(
        changed_axes(before, preferences.fingerprint()),
        [false, false, false, false, false, true]
    );

    let before = preferences.fingerprint();
    preferences.set_appearance(AppearancePreferences {
        ui_size: taskmanager_theme::tokens::UiSize::Large,
        ..preferences.appearance()
    });
    assert_eq!(
        changed_axes(before, preferences.fingerprint()),
        [true, false, false, false, false, false]
    );
}

#[test]
fn whole_snapshot_replace_preserves_unrelated_fingerprints_and_noop_is_inert() {
    let mut preferences = PresentationPreferences::default();
    let initial = preferences.fingerprint();
    preferences.replace(preferences.snapshot());
    assert_eq!(preferences.fingerprint(), initial);

    let mut next = preferences.snapshot();
    next.devices.gpus = false;
    next.graphs.network_dynamic_scaling = false;
    preferences.replace(next);
    assert_eq!(
        changed_axes(initial, preferences.fingerprint()),
        [false, true, false, true, false, false]
    );
}

#[test]
fn window_decorations_mutation_bumps_only_the_appearance_axis_and_noop_is_inert() {
    use taskmanager_core::core::config::{
        WINDOW_DECORATIONS_CUSTOM, WINDOW_DECORATIONS_NATIVE, WINDOW_DECORATIONS_SYSTEM,
    };
    let mut preferences = PresentationPreferences::default();
    assert_eq!(
        preferences.snapshot().window_decorations(),
        WINDOW_DECORATIONS_SYSTEM,
        "the default snapshot carries the System sentinel"
    );

    let before = preferences.fingerprint();
    preferences.set_window_decorations(gpui::SharedString::from(WINDOW_DECORATIONS_CUSTOM));
    assert_eq!(
        changed_axes(before, preferences.fingerprint()),
        [true, false, false, false, false, false],
        "the frame policy persists with the appearance/startup-page axis"
    );
    assert_eq!(
        preferences.snapshot().window_decorations(),
        WINDOW_DECORATIONS_CUSTOM
    );

    // Writing the same token again must not churn the fingerprint, or the
    // periodic save would consider every projection dirty forever.
    let before = preferences.fingerprint();
    preferences.set_window_decorations(gpui::SharedString::from(WINDOW_DECORATIONS_CUSTOM));
    assert_eq!(preferences.fingerprint(), before);

    // A whole-snapshot replace that changes only the frame policy still
    // bumps the appearance axis (the fold path used by config publications).
    let mut next = preferences.snapshot();
    next.window_decorations = gpui::SharedString::from(WINDOW_DECORATIONS_NATIVE);
    let before = preferences.fingerprint();
    preferences.replace(next);
    assert_eq!(
        changed_axes(before, preferences.fingerprint()),
        [true, false, false, false, false, false]
    );
}
