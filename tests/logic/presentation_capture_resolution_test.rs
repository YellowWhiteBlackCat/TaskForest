//! Capture-evidence proof for the shared presentation resolution (ADR-064).
//!
//! A PNG proves a frame was painted. This suite proves the frame was painted
//! with the resolution the shared contract mandates: it reads the published
//! capture markers, parses the recorded presentation facts, and re-derives
//! every value through the same folds the frontend used. A disagreement is a
//! defect, not a style difference.
//!
//! The marker is a TEXTUAL artifact whose contract IS the text, so reading it
//! is evidence work rather than source inspection.

use std::fs;
use std::path::{Path, PathBuf};

use taskmanager_theme::tokens::{RowDensity, UiSize};
use taskmanager_ui_contract::FrontendShape;
use taskmanager_ui_contract::presentation::resolution::{
    ResolvedPresentation, parse_resolution_marker, verify_resolved,
};
use taskmanager_ui_contract::presentation::thresholds::bands;

/// The published evidence roots that carry capture markers.
const EVIDENCE_ROOTS: &[&str] = &[
    "target/screenshot-evidence",
    "target/screenshot-evidence-demo",
    "target/iced-evidence",
    "target/bevy-evidence",
    "target/tui-evidence",
];

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn marker_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(marker_files(&path));
        } else if path.extension().is_some_and(|extension| extension == "log")
            && path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().contains("marker"))
        {
            files.push(path);
        }
    }
    files
}

/// An embedded resolution record begins at `profile=`; the viewport marker's
/// own `rung`/`inventory` fields belong to a different contract and must not
/// overwrite the resolution record when a single capture line carries both.
fn resolution_record(line: &str) -> &str {
    line.find(" profile=")
        .map_or(line, |start| &line[start + 1..])
}

/// One captured resolution line, with the frontend that produced it.
fn captured_resolutions() -> Vec<(FrontendShape, String)> {
    let mut records = Vec::new();
    for root in EVIDENCE_ROOTS {
        for path in marker_files(&repository().join(root)) {
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            for line in text.lines() {
                let Some(frontend) = line
                    .strip_prefix("CAPTURE_MARKER ")
                    .or_else(|| line.strip_prefix("ICED_CAPTURE_MARKER "))
                else {
                    continue;
                };
                // Only a record that carries the shared resolution signature
                // is in scope. A capture published before the resolution
                // contract existed is skipped, not failed: it proves nothing
                // about the contract either way.
                if !frontend.contains("control_extent=") || !frontend.contains("profile=") {
                    continue;
                }
                let shape = if text.contains("ICED_CAPTURE_MARKER")
                    || path.to_string_lossy().contains("iced-evidence")
                {
                    FrontendShape::Iced
                } else {
                    FrontendShape::Gpui
                };
                records.push((shape, resolution_record(frontend).to_owned()));
            }
        }
    }
    records
}

/// The recorded ladder values must be the theme's actual numbers: a frontend
/// that resolves through a hand-rolled value fails here even when its own
/// fold is self-consistent.
fn recorded_ladder_is_a_theme_point(resolved: &ResolvedPresentation) -> bool {
    let ui_size = UiSize::ALL.iter().any(|size| {
        resolved.control_height == size.control_height().0
            && resolved.body_font_size == size.body_font_size().0
            && (resolved.renderer_scale - size.renderer_scale()).abs() <= 0.001
    });
    let density = RowDensity::ALL.iter().any(|density| {
        resolved.row_padding_y == density.row_padding_y().0
            && resolved.line_height == density.line_height().0
    });
    ui_size && density
}

#[test]
fn every_published_capture_resolution_matches_the_shared_contract() {
    let records = captured_resolutions();
    if records.is_empty() {
        eprintln!(
            "skipping published_capture_resolution_matches_the_shared_contract: \
             no published capture carries a presentation resolution yet"
        );
        return;
    }
    let mut failures = Vec::new();
    for (shape, line) in &records {
        let Some(resolved) = parse_resolution_marker(line) else {
            failures.push(format!("{shape:?}: unparsable resolution record: {line}"));
            continue;
        };
        if !recorded_ladder_is_a_theme_point(&resolved) {
            failures.push(format!(
                "{shape:?}: recorded ladder is not a theme point: {line}"
            ));
            continue;
        }
        let findings = verify_resolved(*shape, &resolved, &bands(*shape));
        for finding in findings {
            failures.push(format!("{shape:?}: {:?} in {line}", finding.kind));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn a_tampered_resolution_record_is_rejected() {
    // The gate must be able to see a disagreement: a record that claims a
    // different runway than the shared ladder produces for its own height.
    let shape = FrontendShape::Gpui;
    let shape_bands = bands(shape);
    let record = "profile=standard vertical=standard padding=16.00 width_input=1164.00 \
                  page_content_height=748.00 control_extent=38.00 hit_target=50.29 \
                  control_height=38.00 body_font_size=16.00 renderer_scale=1.1429 \
                  row_padding_y=6.00 line_height=1.40 rung=charts inventory=full details=pinned \
                  device_nav=sidebar sidebar_visible=true persisted_sidebar_width=none \
                  workspace_width=1180.00 content_height=748.00 sidebar_width=260.00 \
                  stats_width=280.00 main_width=628.00";
    let resolved = parse_resolution_marker(record).expect("the record parses");
    assert!(
        verify_resolved(shape, &resolved, &shape_bands).is_empty(),
        "a faithful record must pass"
    );

    let tampered = parse_resolution_marker(&record.replace("rung=charts", "rung=floor"))
        .expect("the record parses");
    let findings = verify_resolved(shape, &tampered, &shape_bands);
    assert!(
        !findings.is_empty(),
        "a rung that contradicts the shared ladder must be rejected"
    );
}
