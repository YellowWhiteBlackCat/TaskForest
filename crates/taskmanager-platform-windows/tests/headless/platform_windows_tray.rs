use super::*;
use taskmanager_core::tray::{TrayIconData, TrayMenuItem, TrayMenuSpec};

// Only consumed by the `cfg(not(target_os = "windows"))` test below, so the
// helper is dead text when the suite is checked against a windows target.
#[cfg_attr(target_os = "windows", allow(dead_code))]
fn spec_with_menu(items: Vec<TrayMenuItem>) -> TraySpec {
    TraySpec::new(
        TrayIconData::from_rgba(vec![0u8; 4 * 4 * 4], 4, 4).unwrap(),
        Some("tip".into()),
        None,
        TrayMenuSpec::from_items(items).unwrap(),
        false,
    )
    .unwrap()
}

#[test]
fn menu_ids_round_trip_for_ours_only() {
    assert_eq!(
        taskmanager_tray_muda::decode_menu_id(&taskmanager_tray_muda::menu_id_for(42)),
        Some(42)
    );
    assert_eq!(taskmanager_tray_muda::decode_menu_id("other-app:7"), None);
}

#[test]
fn spawn_on_non_windows_is_typed_unsupported() {
    #[cfg(not(windows))]
    {
        let (tx, _rx) = std::sync::mpsc::channel();
        let result = spawn_tray(spec_with_menu(Vec::new()), tx);
        assert!(matches!(result, Err(TrayFailure::Unsupported)));
    }
}
