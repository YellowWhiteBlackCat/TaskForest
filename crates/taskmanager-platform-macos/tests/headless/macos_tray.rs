use super::*;
use taskmanager_core::tray::{TrayIconData, TrayMenuItem, TrayMenuSpec};

// Only consumed by the `cfg(not(target_os = "macos"))` test below, so the
// helper is dead text when the suite is checked against a darwin target.
#[cfg_attr(target_os = "macos", allow(dead_code))]
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
fn spawn_on_non_macos_is_typed_unsupported() {
    #[cfg(not(target_os = "macos"))]
    {
        let (tx, _rx) = std::sync::mpsc::channel();
        let result = spawn_tray(spec_with_menu(Vec::new()), tx);
        assert!(matches!(result, Err(TrayFailure::Unsupported)));
    }
}
