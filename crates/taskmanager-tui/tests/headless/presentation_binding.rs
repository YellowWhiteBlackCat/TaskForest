use super::*;

#[test]
fn the_measurement_lift_reads_the_theme_ladder() {
    for size in UiSize::ALL {
        let measured = control_measurement(
            ControlRole::Button,
            ControlState::Normal,
            size,
            RowDensity::Comfortable,
        );
        assert_eq!(measured.extent, size.control_height().0);
        assert_eq!(measured.realized_extent, measured.extent.ceil());
    }
}

#[test]
fn the_motion_lift_reads_the_theme_tokens_and_policy() {
    for role in MotionRole::ALL {
        let normal = motion_resolution(role, MotionPolicy::Normal);
        assert_eq!(
            normal.duration,
            Some(motion_durations().for_class(role.duration_class()))
        );
        assert_eq!(
            motion_resolution(role, MotionPolicy::NoMotion).duration,
            None
        );
    }
}
