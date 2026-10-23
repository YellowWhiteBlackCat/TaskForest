//! Headless behavior tests for the diagnostics report skeleton (ICED-050):
//! the placeholder is driven by the REAL in-flight state, so it appears
//! exactly while the report is being written and never over a preview, a
//! finished report or a failure.

use super::*;
use taskmanager_application::diagnostics::DiagnosticBundleUiState;
use taskmanager_core::DiagnosticBundlePlan;
use taskmanager_core::DiagnosticSource;
use taskmanager_core::core::diagnostics::DiagnosticBundleError;
use taskmanager_core::core::diagnostics::DiagnosticBundleErrorKind;
use taskmanager_core::core::diagnostics::{DiagnosticPreview, RedactionSummary};

fn preview() -> DiagnosticPreview {
    DiagnosticPreview {
        files: Vec::new(),
        total_bytes: 0,
        redactions: RedactionSummary {
            usernames: 0,
            paths: 0,
            ipv4_addresses: 0,
            ipv6_addresses: 0,
        },
        manifest_sha256: String::new(),
    }
}

/// The placeholder belongs to the IN-FLIGHT state only: the state the
/// surface reaches after the export was confirmed and before the write
/// settles.
#[test]
fn the_skeleton_appears_only_for_the_in_flight_write() {
    let writing = DiagnosticBundleUiState::Writing(preview());
    let skeleton =
        diagnostic_skeleton(&writing).expect("the in-flight state carries the placeholder");
    assert_eq!(skeleton.rows.len(), DIAGNOSTIC_SKELETON_ROWS);
}

/// A preview, a finished report and a failure all render their own
/// content: the placeholder never covers real information.
#[test]
fn the_skeleton_never_covers_a_preview_a_result_or_a_failure() {
    let plan = DiagnosticBundleUiState::prepared(DiagnosticBundlePlan::prepare(
        vec![DiagnosticSource {
            name: "service-logs.json".into(),
            contents: "{}".into(),
        }],
        [],
    ));
    assert!(
        diagnostic_skeleton(&plan).is_none(),
        "a preview is real content, never a placeholder"
    );
    assert!(
        diagnostic_skeleton(&DiagnosticBundleUiState::Complete(
            std::path::PathBuf::from("/tmp/report.json")
        ))
        .is_none(),
        "a finished report is real content, never a placeholder"
    );
    assert!(
        diagnostic_skeleton(&DiagnosticBundleUiState::Failed(
            DiagnosticBundleError::new(DiagnosticBundleErrorKind::Unavailable)
        ))
        .is_none(),
        "a failure is real content, never a placeholder"
    );
}

/// The placeholder's bands taper like the preview's own lines and carry
/// their own wave phase.
#[test]
fn the_placeholder_bands_taper_and_carry_their_wave_phase() {
    let skeleton = diagnostic_skeleton(&DiagnosticBundleUiState::Writing(preview()))
        .expect("the in-flight state carries the placeholder");
    for (index, band) in skeleton.rows.iter().enumerate() {
        assert!(
            band.width > 0.0 && band.width <= 1.0,
            "a band takes a real share of the row"
        );
        assert!(
            (0.0..1.0).contains(&band.phase),
            "a band's phase stays on the wave"
        );
        if index > 0 {
            assert!(
                band.width < skeleton.rows[index - 1].width,
                "the bands taper like the preview's lines"
            );
        }
    }
}

/// The wave is a smooth, bounded, periodic band: its brightness never
/// leaves the range the placeholder's alpha is derived from.
#[test]
fn the_wave_stays_inside_its_range_and_is_periodic() {
    for step in 0..=64 {
        let phase = step as f32 / 64.0;
        let wave = skeleton_wave(phase);
        assert!(
            (0.0..=1.0).contains(&wave),
            "the wave stays inside 0..1, got {wave}"
        );
        assert!(
            (skeleton_wave(phase) - skeleton_wave(phase + 1.0)).abs() < 1e-6,
            "the wave repeats every cycle"
        );
    }
    assert!(
        (skeleton_wave(0.25) - skeleton_wave(1.25)).abs() < 1e-6,
        "the wave repeats every cycle"
    );
}

/// The app's skeleton view follows the surface: an in-flight report shows
/// the placeholder, any other surface shows nothing.
#[test]
fn the_app_skeleton_view_follows_the_real_surface_state() {
    let mut app = crate::IcedApp::demo();
    assert!(
        app.diagnostic_skeleton_view().is_none(),
        "no diagnostics surface: no placeholder"
    );
    app.open_local_surface(LocalSurface::DiagnosticBundle(
        DiagnosticBundleUiState::Writing(preview()),
    ));
    assert!(
        app.diagnostic_skeleton_view().is_some(),
        "the in-flight report shows the placeholder"
    );
    app.open_local_surface(LocalSurface::DiagnosticBundle(
        DiagnosticBundleUiState::Complete(std::path::PathBuf::from("/tmp/report.json")),
    ));
    assert!(
        app.diagnostic_skeleton_view().is_none(),
        "a finished report shows its own content"
    );
}
