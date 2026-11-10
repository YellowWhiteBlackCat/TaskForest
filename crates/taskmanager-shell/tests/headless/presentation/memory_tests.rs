use super::*;

#[test]
fn the_shared_used_rate_floor_matches_the_legacy_predicate() {
    for step in -100_i16..=100 {
        let rate = f32::from(step) * 0.01;
        assert_eq!(
            memory_usage_rate_is_visible(rate),
            rate.abs() >= 0.05,
            "{rate}"
        );
    }
    assert!(!memory_usage_rate_is_visible(f32::NAN));
    assert!(!memory_usage_rate_is_visible(f32::INFINITY));
    assert!(!memory_usage_rate_is_visible(f32::NEG_INFINITY));
    assert!(memory_usage_rate_is_visible(0.05));
    assert!(memory_usage_rate_is_visible(-0.05));
}
