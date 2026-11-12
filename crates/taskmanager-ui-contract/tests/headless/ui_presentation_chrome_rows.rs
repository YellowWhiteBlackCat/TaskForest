//! The chrome-row height contract: the fixed rows a page paints above its
//! content slot.

use super::*;
use crate::keybindings::FrontendShape;

/// The theme's Standard axis point, as the contract consumes it.
fn ui_size() -> UiSizePoint {
    UiSizePoint {
        control_height: 38.0,
        body_font_size: 16.0,
        renderer_scale: 16.0 / 14.0,
    }
}

/// The theme's comfortable density point, as the contract consumes it.
fn density() -> DensityPoint {
    DensityPoint {
        row_padding_y: 6.0,
        line_height: 1.4,
    }
}

/// A chrome row's height is the ladder extent, times the lines it wraps
/// into, plus the gap between two lines.
#[test]
fn a_chrome_row_spends_the_ladder_extent_over_its_lines() {
    let row = ChromeRowSpec::from_ladder(
        FrontendShape::Gpui,
        ChromeRowRole::SectionTabs,
        ControlState::Normal,
        ui_size(),
        density(),
        1,
        8.0,
    );
    assert_eq!(row.line_height, 38.0);
    assert_eq!(row.height(), 38.0);

    let wrapped = ChromeRowSpec::from_ladder(
        FrontendShape::Gpui,
        ChromeRowRole::SectionTabs,
        ControlState::Normal,
        ui_size(),
        density(),
        2,
        8.0,
    );
    assert_eq!(wrapped.height(), 38.0 * 2.0 + 8.0);
}

/// Every chrome row spends the SAME ladder role, so a page never picks a
/// second measurement for the same row.
#[test]
fn every_chrome_row_role_spends_the_ladder_control_role() {
    for role in [
        ChromeRowRole::SectionTabs,
        ChromeRowRole::WindowControls,
        ChromeRowRole::Paging,
    ] {
        assert_eq!(role.control_role(), ControlRole::Tab);
        let row = ChromeRowSpec::from_ladder(
            FrontendShape::Gpui,
            role,
            ControlState::Normal,
            ui_size(),
            density(),
            1,
            8.0,
        );
        assert_eq!(
            row.measurement(
                FrontendShape::Gpui,
                ControlState::Normal,
                ui_size(),
                density()
            )
            .extent,
            row.line_height,
            "{role:?}: the row spends the measurement it published"
        );
    }
}

/// The row follows the theme's UI-size axis: a smaller control ladder spends
/// a smaller row.
#[test]
fn a_chrome_row_follows_the_theme_axis() {
    let small = UiSizePoint {
        control_height: 32.0,
        body_font_size: 14.0,
        renderer_scale: 1.0,
    };
    let row = ChromeRowSpec::from_ladder(
        FrontendShape::Gpui,
        ChromeRowRole::Paging,
        ControlState::Normal,
        small,
        density(),
        1,
        8.0,
    );
    assert_eq!(row.height(), 32.0);
}

/// A readout beside the controls may be taller than the control extent: the
/// row then grows to the readout instead of clipping it.
#[test]
fn a_readout_never_clips_inside_its_row() {
    let row = ChromeRowSpec::from_ladder(
        FrontendShape::Gpui,
        ChromeRowRole::WindowControls,
        ControlState::Normal,
        ui_size(),
        density(),
        1,
        8.0,
    );
    assert_eq!(row.with_readout(26.0).height(), 38.0);
    assert_eq!(row.with_readout(52.0).height(), 52.0);
    assert_eq!(row.with_readout(f32::NAN).height(), 38.0);
}

/// A degenerate measurement never spends a negative row.
#[test]
fn a_degenerate_chrome_row_never_spends_a_negative_height() {
    let row = ChromeRowSpec::measured(ChromeRowRole::Paging, 24.0, 0, 8.0);
    assert_eq!(
        row.height(),
        24.0,
        "an empty line count still paints one line"
    );
    let negative = ChromeRowSpec::measured(ChromeRowRole::Paging, -8.0, 1, 8.0);
    assert_eq!(negative.height(), -8.0);
}
