use super::*;
use taskmanager_theme::Theme;

/// The tier bands are the cell's headline rule: green below 60, amber
/// 60–85, red above. Asserted at the band edges so a future threshold edit
/// does not silently shift a pinned core's color.
#[test]
fn tier_color_bands_match_the_grid_rule() {
    let theme = Theme::dark();
    assert_eq!(
        tier_color(&theme, 0.0),
        crate::theme_binding::color(theme.success)
    );
    assert_eq!(
        tier_color(&theme, 59.9),
        crate::theme_binding::color(theme.success)
    );
    assert_eq!(
        tier_color(&theme, 60.0),
        crate::theme_binding::color(theme.warning)
    );
    assert_eq!(
        tier_color(&theme, 84.9),
        crate::theme_binding::color(theme.warning)
    );
    assert_eq!(
        tier_color(&theme, 85.0),
        crate::theme_binding::color(theme.danger)
    );
    assert_eq!(
        tier_color(&theme, 100.0),
        crate::theme_binding::color(theme.danger)
    );
}

/// Out-of-range readings clamp into the bands — a negative load stays green
/// and an over-100 spike reads red, never an out-of-band token.
#[test]
fn tier_color_clamps_out_of_range_readings_into_the_bands() {
    let theme = Theme::dark();
    assert_eq!(
        tier_color(&theme, -50.0),
        crate::theme_binding::color(theme.success)
    );
    assert_eq!(
        tier_color(&theme, 250.0),
        crate::theme_binding::color(theme.danger)
    );
}

/// A core window with one finite sample yields no strokeable polyline — the
/// honest first-snapshot state (the readout carries the value until a
/// second snapshot arrives). This reuses the shared projection's contract,
/// so the assertion is the same one `perf_chart` makes.
#[test]
fn single_sample_core_window_strokes_no_polyline() {
    let size = iced::Size::new(40.0, 24.0);
    let one = series_point_runs(&[42.0], size);
    assert_eq!(one[0].len(), 1);
    assert!(line_path(&one[0]).is_none(), "one sample must not stroke");
    let two = series_point_runs(&[42.0, 55.0], size);
    assert!(line_path(&two[0]).is_some(), "two samples must stroke");
}

/// The fingerprint retains the immutable per-core snapshot generation, so a
/// shifted window cannot hide behind the same length and tail value.
#[test]
fn core_cell_fingerprint_keys_on_snapshot_generation() {
    let samples: Rc<[f32]> = Rc::from([10.0, 50.0, 90.0].as_slice());
    let base = CoreCellFingerprint::from_samples(&samples);
    assert_eq!(base, CoreCellFingerprint::from_samples(&samples));
    let shifted: Rc<[f32]> = Rc::from([70.0, 50.0, 90.0].as_slice());
    assert_ne!(base, CoreCellFingerprint::from_samples(&shifted));
}

/// The program's `fingerprint()` mirrors `CoreCellFingerprint::from_samples`
/// — the seam `draw()` keys the cache-clear gate on — so a core whose
/// window did not change reuses last frame's geometry. The stroke color
/// (tier token) is NOT in the fingerprint: a theme switch is rare and one
/// stale-color frame is acceptable (matches round-1 process_sparkline;
/// asserted here so it is not "fixed" back).
#[test]
fn core_cell_program_fingerprint_tracks_history_not_color() {
    let theme = Theme::dark();
    let green = tier_color(&theme, 30.0);
    let red = tier_color(&theme, 90.0);
    let samples: Rc<[f32]> = Rc::from([10.0, 20.0, 30.0].as_slice());
    let chart = CoreCellChart::new(Rc::clone(&samples), green);
    // Same data + same color → same fingerprint.
    assert_eq!(
        chart.fingerprint(),
        CoreCellChart::new(Rc::clone(&samples), green).fingerprint()
    );
    // Same data but a different tier color → SAME fingerprint (color is not
    // part of the fingerprint).
    assert_eq!(
        chart.fingerprint(),
        CoreCellChart::new(Rc::clone(&samples), red).fingerprint()
    );
    // A new tick → different fingerprint (cache clears).
    assert_ne!(
        chart.fingerprint(),
        CoreCellChart::new(Rc::from([10.0, 20.0, 40.0].as_slice()), green).fingerprint()
    );
}

#[test]
fn format_core_readout_surfaces_clock_frequency() {
    assert_eq!(format_core_readout(Some(35.2), None), " 35%");
    assert_eq!(format_core_readout(Some(50.0), Some(800)), " 50% · 800 MHz");
    assert_eq!(
        format_core_readout(Some(75.8), Some(3600)),
        " 76% · 3.6 GHz"
    );
}

#[test]
fn format_core_readout_keeps_missing_and_non_finite_usage_honest() {
    assert_eq!(format_core_readout(None, None), MISSING_VALUE);
    assert_eq!(format_core_readout(Some(f32::NAN), None), MISSING_VALUE);
    assert_eq!(
        format_core_readout(Some(f32::INFINITY), None),
        MISSING_VALUE
    );
    assert_eq!(
        format_core_readout(Some(f32::NAN), Some(1800)),
        "1.8 GHz",
        "an unavailable utilization must not hide an independently observed clock"
    );
}

/// The batched canvas and the text cells must resolve the SAME cell slices:
/// the bands tile the row with the row's own gap and no overlap, so a
/// batched sparkline can never drift off its label.
#[test]
fn the_cell_bands_tile_the_row_without_gap_or_overlap() {
    let row_width = 600.0;
    let count = 6;

    let mut cursor = 0.0;
    for index in 0..count {
        let band = cell_chart_rect(row_width, count, index);
        assert!(
            (band.x - cursor).abs() < 1e-3,
            "cell {index} starts where the previous one ended: {band:?}"
        );
        cursor = band.x + band.width + ROW_SPACING;
    }
    let last = cell_chart_rect(row_width, count, count - 1);
    assert!(
        (last.x + last.width - row_width).abs() < 1e-3,
        "the last cell ends at the row's right edge: {last:?}"
    );
}

/// The band starts below the pinned label line: the label, the gap and the
/// band are the same three numbers on both sides of the grid.
#[test]
fn the_batched_band_starts_below_the_pinned_label() {
    let band = cell_chart_rect(600.0, 6, 0);

    assert!(
        (band.y - (label_height() + CELL_SPACING)).abs() < 1e-3,
        "the band starts under the label: {band:?}"
    );
    assert!(
        (band.height - CELL_CHART_HEIGHT).abs() < 1e-3,
        "the band is the cell's fixed sparkline height: {band:?}"
    );
}

/// A many-core host pays per ROW, not per core: the grid batches a row's
/// sparklines into one canvas, so the submission count follows the row count.
#[test]
fn a_many_core_host_batches_its_canvases_per_row() {
    assert_eq!(grid_row_count(6), 1);
    assert_eq!(grid_row_count(7), 2);
    assert_eq!(grid_row_count(128), 22);
    assert!(
        grid_row_count(128) < 128,
        "the batched grid submits fewer canvases than it has cores"
    );
}
