use super::*;
use taskmanager_core::{ScalarAvailability, ThermalCoolingActivity, ThermalTripKind};

fn fixture(name: &str) -> PathBuf {
    crate::test_support::repo_temp_dir().join(format!(
        "tm-thermal-control-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ))
}

#[test]
fn trip_fields_keep_independent_availability_and_reject_firmware_sentinel() {
    let root = fixture("trips");
    fs::create_dir_all(&root).expect("zone directory");
    fs::write(root.join("trip_point_0_type"), "critical\n").expect("trip kind");
    fs::write(root.join("trip_point_0_temp"), "105000\n").expect("trip temperature");
    fs::write(root.join("trip_point_0_hyst"), "not-a-number\n").expect("trip hysteresis");
    fs::write(root.join("trip_point_4_type"), "passive\n").expect("sentinel kind");
    fs::write(root.join("trip_point_4_temp"), "-274000\n").expect("firmware sentinel");
    fs::write(root.join("trip_point_4_hyst"), "2000\n").expect("sentinel hysteresis");

    let trips = collect_trip_points(&root, 20);

    assert_eq!(trips.current_points().map(<[_]>::len), Some(2));
    assert_eq!(
        trips.points[0].kind.current_value(),
        Some(&ThermalTripKind::Critical)
    );
    assert_eq!(
        trips.points[0].temperature_millicelsius.current_value(),
        Some(&105_000)
    );
    assert_eq!(
        trips.points[0].hysteresis_millicelsius.availability(),
        ScalarAvailability::Unavailable(FailureKind::ProviderFault)
    );
    assert_eq!(
        trips.points[1].temperature_millicelsius.availability(),
        ScalarAvailability::Unavailable(FailureKind::ProviderFault)
    );
    assert_eq!(
        trips.points[1].hysteresis_millicelsius.current_value(),
        Some(&2_000)
    );
    fs::remove_dir_all(root).ok();
}

#[test]
fn cooling_state_is_ordinal_and_activity_requires_consistent_bounds() {
    let root = fixture("cooling");
    fs::create_dir_all(&root).expect("cooling directory");
    fs::write(root.join("cur_state"), "5\n").expect("current state");
    fs::write(root.join("max_state"), "7\n").expect("maximum state");
    let type_name = Ok("CHRG".to_owned());

    let current = collect_cooling_device(
        &root,
        "cooling:type:CHRG:channel".into(),
        DeviceId::new("cooling:type:CHRG"),
        &type_name,
        30,
    );
    assert_eq!(current.current_state.current_value(), Some(&5));
    assert_eq!(current.maximum_state.current_value(), Some(&7));
    assert_eq!(
        current.activity.current_value(),
        Some(&ThermalCoolingActivity::Active)
    );

    fs::write(root.join("cur_state"), "9\n").expect("inconsistent state");
    let inconsistent = collect_cooling_device(
        &root,
        "cooling:type:CHRG:channel".into(),
        DeviceId::new("cooling:type:CHRG"),
        &type_name,
        40,
    );
    assert_eq!(inconsistent.current_state.current_value(), Some(&9));
    assert_eq!(inconsistent.maximum_state.current_value(), Some(&7));
    assert_eq!(
        inconsistent.activity.availability(),
        ScalarAvailability::Unavailable(FailureKind::ProviderFault)
    );
    fs::remove_dir_all(root).ok();
}

#[cfg(target_os = "linux")]
#[test]
fn denied_zone_field_is_typed_without_erasing_readable_siblings() {
    use std::os::unix::fs::PermissionsExt;

    let root = fixture("permission");
    fs::create_dir_all(&root).expect("zone directory");
    fs::write(root.join("mode"), "enabled\n").expect("zone mode");
    fs::write(root.join("policy"), "step_wise\n").expect("zone policy");
    fs::set_permissions(root.join("mode"), fs::Permissions::from_mode(0o000))
        .expect("deny mode field");
    let type_name = Ok("fixture-zone".to_owned());

    let zone = collect_zone(
        &root,
        "thermal:type:fixture-zone:zone".into(),
        DeviceId::new("thermal:type:fixture-zone"),
        &type_name,
        50,
    );
    fs::set_permissions(root.join("mode"), fs::Permissions::from_mode(0o600))
        .expect("restore mode field");

    assert_eq!(
        zone.mode.availability(),
        ScalarAvailability::Unavailable(FailureKind::PermissionDenied)
    );
    assert_eq!(zone.policy.current_value(), Some(&ThermalPolicy::StepWise));
    assert_eq!(zone.trip_points.current_points(), Some([].as_slice()));
    fs::remove_dir_all(root).ok();
}
