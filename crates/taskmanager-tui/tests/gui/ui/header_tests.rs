use super::bounded_header_label;
use super::header_support::header_tab_text;
use taskmanager_ui_contract::IconId;

#[test]
fn narrow_header_uses_identity_without_keyboard_hint_overflow() {
    let full = header_tab_text(IconId::History, "App history", "Alt+7", 140);
    let compact = header_tab_text(IconId::History, "App history", "Alt+7", 100);
    let narrow = header_tab_text(IconId::History, "App history", "Alt+7", 72);
    let tiny = header_tab_text(IconId::History, "App history", "Alt+7", 60);

    assert!(full.contains("Alt+7"));
    assert!(!compact.contains("Alt+7"));
    assert!(narrow.contains("App h…"));
    assert!(!tiny.contains("App history"));
    assert!(!tiny.contains("Alt+7"));
}

#[test]
fn header_label_bound_preserves_short_labels_and_marks_truncation() {
    assert_eq!(bounded_header_label("CPU", 6), "CPU");
    assert_eq!(bounded_header_label("Applications", 6), "Appli…");
}
