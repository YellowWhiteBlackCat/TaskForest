//! Headless tests for the shared secondary-series tint law.

use super::*;

#[test]
fn the_law_lifts_every_channel_strictly_toward_white() {
    for channel in [0.0_f32, 0.18, 0.5, 0.9, 1.0] {
        let lifted = tint_channel_toward_white(channel);
        assert!(lifted >= channel, "lift must never darken {channel}");
        assert!(lifted <= 1.0, "lift must never overshoot white");
        if channel < 1.0 {
            assert!(lifted > channel, "a mid channel must move toward white");
        }
    }
}

#[test]
fn white_stays_white_and_black_lands_on_the_factor() {
    assert_eq!(tint_channel_toward_white(1.0), 1.0);
    assert!((tint_channel_toward_white(0.0) - SECONDARY_TINT_LIFT).abs() < 1e-6);
}
