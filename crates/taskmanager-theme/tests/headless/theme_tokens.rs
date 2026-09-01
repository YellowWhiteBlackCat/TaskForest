use std::time::Duration;

use super::{
    DURATION_FAST, DURATION_HOVER, DURATION_MEDIUM, LINE_HEIGHT_NORMAL, Length, MotionPolicy,
    RowDensity, UiSize,
};

#[test]
fn compact_rows_are_tighter_than_comfortable_in_every_axis() {
    // Compact must reduce the vertical padding on both rows and header…
    assert!(RowDensity::Compact.row_padding_y() < RowDensity::Comfortable.row_padding_y());
    assert!(RowDensity::Compact.header_padding_y() < RowDensity::Comfortable.header_padding_y());
    // …and tighten the body line-height below the comfortable ratio.
    assert!(RowDensity::Compact.line_height() != LINE_HEIGHT_NORMAL);
    // Comfortable is the identity: it matches the pre-density look exactly
    // (SPACE_6 vertical padding, LINE_HEIGHT_NORMAL leading) so existing
    // baselines and call sites are unchanged until a user opts in.
    assert_eq!(RowDensity::Comfortable.row_padding_y(), Length(6.0));
    assert_eq!(RowDensity::Comfortable.header_padding_y(), Length(6.0));
    assert_eq!(RowDensity::Comfortable.line_height(), LINE_HEIGHT_NORMAL);
    // Compact stays >= 2px padding so dense rows never clip the glyphs.
    assert!(RowDensity::Compact.row_padding_y() >= Length(2.0));
}

#[test]
fn ui_size_is_a_monotonic_readability_axis_independent_from_density() {
    assert_eq!(UiSize::default(), UiSize::Standard);
    assert_eq!(UiSize::ALL.len(), 3);
    assert!(UiSize::Small.body_font_size() < UiSize::Standard.body_font_size());
    assert!(UiSize::Standard.body_font_size() < UiSize::Large.body_font_size());
    assert!(UiSize::Small.icon_size() < UiSize::Standard.icon_size());
    assert!(UiSize::Standard.icon_size() < UiSize::Large.icon_size());
    assert!(UiSize::Small.control_height() < UiSize::Standard.control_height());
    assert!(UiSize::Standard.control_height() < UiSize::Large.control_height());
    assert_eq!(UiSize::Small.renderer_scale(), 1.0);
    assert!(UiSize::Small.renderer_scale() < UiSize::Standard.renderer_scale());
    assert!(UiSize::Standard.renderer_scale() < UiSize::Large.renderer_scale());
    assert_eq!(UiSize::Standard.caption_font_size(), Length(12.0));

    // Row density remains an orthogonal whitespace/leading choice.
    assert_eq!(UiSize::Standard.body_font_size(), Length(16.0));
    assert!(RowDensity::Compact.row_padding_y() < RowDensity::Comfortable.row_padding_y());
}

#[test]
fn ui_size_config_tokens_round_trip_and_unknown_input_falls_back_to_standard() {
    for size in UiSize::ALL {
        assert_eq!(UiSize::from_config_token(size.config_token()), size);
    }
    assert_eq!(UiSize::from_config_token(""), UiSize::Standard);
    assert_eq!(UiSize::from_config_token(" FutureScale "), UiSize::Standard);
}

#[test]
fn motion_policy_has_explicit_normal_reduced_and_no_motion_semantics() {
    assert_eq!(MotionPolicy::default(), MotionPolicy::Normal);
    assert_eq!(MotionPolicy::ALL.len(), 3);
    assert!(MotionPolicy::Normal.allows_animation());
    assert!(MotionPolicy::Reduced.allows_animation());
    assert!(!MotionPolicy::NoMotion.allows_animation());

    assert_eq!(
        MotionPolicy::Normal.animation_duration(DURATION_HOVER),
        Some(DURATION_HOVER)
    );
    assert_eq!(
        MotionPolicy::Reduced.animation_duration(DURATION_HOVER),
        Some(DURATION_FAST)
    );
    assert_eq!(
        MotionPolicy::Reduced.animation_duration(DURATION_MEDIUM),
        Some(DURATION_FAST)
    );
    assert_eq!(
        MotionPolicy::Reduced.animation_duration(DURATION_FAST),
        Some(DURATION_FAST)
    );
    assert_eq!(
        MotionPolicy::NoMotion.animation_duration(DURATION_MEDIUM),
        None
    );
    assert_eq!(
        MotionPolicy::Normal.animation_duration(Duration::ZERO),
        None
    );
}
