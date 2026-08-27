use super::*;

#[test]
fn columns_fit_the_terminal_width() {
    assert_eq!(columns_for(54), 2);
    assert_eq!(columns_for(120), 6);
    assert_eq!(columns_for(10), 1);
}

#[test]
fn no_samples_renders_an_honest_dash_never_zero() {
    let cell = core_cell(&[]);
    assert_eq!(cell.filled, 0);
    assert_eq!(cell.readout, "—");
    assert!(cell.utilization.is_none());
}

#[test]
fn full_scale_renders_a_full_bar_and_clamps_are_honest() {
    let half = core_cell(&[50.0]);
    assert_eq!(half.filled, 4);
    assert_eq!(half.readout, " 50%");
    let clamped = core_cell(&[150.0]);
    assert_eq!(clamped.filled, BAR_CHARS);
    assert_eq!(clamped.readout, "100%");
}

/// A pinned core (≥85%) wears the danger color, a busy one (≥60%) the warn
/// color, an idle one the good color — so hotspots are scannable at a glance.
#[test]
fn tier_color_tracks_the_load_band() {
    let theme = TuiTheme::default();
    assert_eq!(tier_color(theme, None), theme.dim);
    assert_eq!(tier_color(theme, Some(5.0)), theme.good);
    assert_eq!(tier_color(theme, Some(60.0)), theme.warn);
    assert_eq!(tier_color(theme, Some(85.0)), theme.danger);
    assert_eq!(tier_color(theme, Some(99.0)), theme.danger);
    // Out-of-range values clamp into the nearest band before tinting.
    assert_eq!(tier_color(theme, Some(150.0)), theme.danger);
}
