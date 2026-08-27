use super::*;
use taskmanager_shell::demo_app;

#[test]
fn about_modal_renders_fixture_hardware_and_snapshot_facts() {
    use taskmanager_application::i18n::{Language, set_language};
    set_language(Language::En);
    let app = crate::IcedApp::demo();
    let _view = render(&app);

    let shell = demo_app();
    let rows = about_rows(
        shell.projection().hardware.as_ref(),
        shell.projection().snapshot.as_ref(),
    );
    assert_eq!(
        rows.iter()
            .find(|row| row.label == t("system.hostname"))
            .map(|row| row.value.as_str()),
        Some("taskforest-workstation")
    );
    assert_eq!(
        rows.iter()
            .find(|row| row.label == t("common.logical_cores"))
            .map(|row| row.value.as_str()),
        Some("22")
    );
    assert_eq!(
        rows.iter()
            .find(|row| row.label == t("common.uptime"))
            .map(|row| row.value.as_str()),
        Some("06h 42m")
    );
}

#[test]
fn about_modal_renders_dashes_when_facts_are_absent() {
    let shell = taskmanager_shell::ShellApp::new();
    let rows = about_rows(
        shell.projection().hardware.as_ref(),
        shell.projection().snapshot.as_ref(),
    );
    assert_eq!(rows.len(), 12);
    assert!(rows.iter().all(|row| row.value == "—"));
}

/// The copy-details payload (G-16) carries the version line plus every
/// rendered row — the same facts the modal shows, never a second source.
#[test]
fn copy_payload_carries_the_version_and_every_rendered_row() {
    use taskmanager_application::i18n::{Language, set_language};
    set_language(Language::En);
    let shell = demo_app();
    let rows = about_rows(
        shell.projection().hardware.as_ref(),
        shell.projection().snapshot.as_ref(),
    );
    let payload = about_copy_payload(
        shell.projection().hardware.as_ref(),
        shell.projection().snapshot.as_ref(),
    );
    assert!(
        payload.starts_with("TaskForestI "),
        "the version line leads the payload: {payload}"
    );
    for row in &rows {
        assert!(
            payload.contains(&format!("{}: {}", row.label, row.value)),
            "row {row:?} must appear in the payload"
        );
    }
    assert_eq!(payload.lines().count(), rows.len() + 1);

    // Absent facts copy honestly as the same dash rows the modal renders.
    let empty = about_copy_payload(None, None);
    assert!(empty.contains("—"));
}

/// The copy action records the footer feedback through the real update
/// path (the clipboard Task itself is runtime-side; the observable state
/// and the payload seam carry the behavior, G-16).
#[test]
fn copy_about_details_message_records_the_footer_feedback() {
    use taskmanager_application::i18n::{Language, set_language};
    set_language(Language::En);
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::OpenAbout);
    assert!(app.about_open());
    assert_ne!(
        app.shell.feedback_notice().map(|notice| notice.source()),
        Some(taskmanager_shell::FeedbackSource::Clipboard)
    );
    let _ = app.update(Message::CopyAboutDetails);
    let feedback = app.shell.feedback_notice().expect("feedback recorded");
    assert_eq!(
        feedback.source(),
        taskmanager_shell::FeedbackSource::Clipboard
    );
    assert!(
        feedback.text().contains("Copied"),
        "feedback: {}",
        feedback.text()
    );
}
