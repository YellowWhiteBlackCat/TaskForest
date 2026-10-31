use super::*;

#[test]
fn normalization_keeps_raw_values_and_scales_by_logical_processors() {
    let load = SystemLoadAverage::from_raw(8.0, 4.0, 2.0, 4).expect("valid load");
    assert_eq!(load.one_minute, 8.0);
    assert_eq!(load.normalized_one_minute, 2.0);
    assert_eq!(load.normalized_five_minutes, 1.0);
    assert_eq!(load.normalized_fifteen_minutes, 0.5);
    assert_eq!(load.logical_processors, 4);
    assert_eq!(load.normalized_peak(), 2.0);
}

#[test]
fn normalization_prefers_physical_cores_and_keeps_the_denominator_explicit() {
    let load = SystemLoadAverage::from_raw_with_physical(8.0, 4.0, 2.0, 8, Some(4))
        .expect("valid physical topology");
    assert_eq!(load.normalized_one_minute, 2.0);
    assert_eq!(load.logical_processors, 8);
    assert_eq!(load.physical_cores, Some(4));
    assert_eq!(load.normalization_processors(), 4);
}

#[test]
fn invalid_loads_are_not_coerced_to_zero() {
    assert!(SystemLoadAverage::from_raw(-1.0, 0.0, 0.0, 4).is_none());
    assert!(SystemLoadAverage::from_raw(f32::NAN, 0.0, 0.0, 4).is_none());
    assert!(SystemLoadAverage::from_raw(0.0, f32::INFINITY, 0.0, 4).is_none());
    assert!(SystemLoadAverage::from_raw(0.0, 0.0, 0.0, 0).is_none());
}
