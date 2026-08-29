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
        taskmanager_theme::iced::color(theme.success)
    );
    assert_eq!(
        tier_color(&theme, 59.9),
        taskmanager_theme::iced::color(theme.success)
    );
    assert_eq!(
        tier_color(&theme, 60.0),
        taskmanager_theme::iced::color(theme.warning)
    );
    assert_eq!(
        tier_color(&theme, 84.9),
        taskmanager_theme::iced::color(theme.warning)
    );
    assert_eq!(
        tier_color(&theme, 85.0),
        taskmanager_theme::iced::color(theme.danger)
    );
    assert_eq!(
        tier_color(&theme, 100.0),
        taskmanager_theme::iced::color(theme.danger)
    );
}

/// Out-of-range readings clamp into the bands — a negative load stays green
/// and an over-100 spike reads red, never an out-of-band token.
#[test]
fn tier_color_clamps_out_of_range_readings_into_the_bands() {
    let theme = Theme::dark();
    assert_eq!(
        tier_color(&theme, -50.0),
        taskmanager_theme::iced::color(theme.success)
    );
    assert_eq!(
        tier_color(&theme, 250.0),
        taskmanager_theme::iced::color(theme.danger)
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
