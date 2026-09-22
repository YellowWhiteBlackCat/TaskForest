//! First-frame capture-marker behavior for the health-modal evidence lane.
//!
//! The marker validator is fail-closed on the declared `device` token, so the
//! contract these tests prove is the one the runner consumes: a health capture
//! frame emits exactly one `device=health` target marker (and no Performance
//! device token), while every other demo frame keeps the device/page branch.

use super::*;
use crate::app::Message;
use std::time::Instant;

/// One health-modal frame: the marker names the local surface and never the
/// Performance device underneath it, exactly once even across frames.
#[test]
fn health_capture_frame_emits_one_health_target_marker() {
    let dir = crate::test_support::temp_dir("iced-health-capture-marker");
    let marker = dir.join("markers.log");
    let mut app = IcedApp::demo();
    app.capture.marker = Some(marker.clone());
    let _ = app.update(Message::OpenHealth);

    let _ = app.handle_window_message(Message::Frame(Instant::now()));
    let _ = app.handle_window_message(Message::Frame(Instant::now()));

    let log = std::fs::read_to_string(&marker).expect("capture marker log");
    assert!(log.contains("ICED_CAPTURE_MARKER event=frame_ready mode=demo page=performance"));
    assert!(log.contains(
        "ICED_CAPTURE_MARKER event=target_ready mode=demo page=performance device=health"
    ));
    assert_eq!(log.matches("event=target_ready").count(), 1);
    assert!(!log.contains("device=cpu"));
    std::fs::remove_dir_all(&dir).expect("clean up capture marker scratch");
}

/// The device branch stays authoritative for every other demo capture.
#[test]
fn performance_capture_frame_keeps_the_selected_device_marker() {
    let dir = crate::test_support::temp_dir("iced-device-capture-marker");
    let marker = dir.join("markers.log");
    let mut app = IcedApp::demo();
    app.capture.marker = Some(marker.clone());

    let _ = app.handle_window_message(Message::Frame(Instant::now()));

    let log = std::fs::read_to_string(&marker).expect("capture marker log");
    assert!(
        log.contains(
            "ICED_CAPTURE_MARKER event=target_ready mode=demo page=performance device=cpu"
        )
    );
    std::fs::remove_dir_all(&dir).expect("clean up capture marker scratch");
}

/// Only a marker-carrying health frame bounds the modal body: a production
/// modal and a marker-less health surface keep the user's own scroll.
#[test]
fn only_a_capture_health_frame_bounds_the_modal_body() {
    let mut app = IcedApp::demo();
    assert!(!app.health_capture_frame());

    let _ = app.update(Message::OpenHealth);
    assert!(
        !app.health_capture_frame(),
        "a production health modal is never scrolled by the frontend"
    );

    app.capture.marker = Some(std::path::PathBuf::from("markers.log"));
    assert!(app.health_capture_frame());
}
