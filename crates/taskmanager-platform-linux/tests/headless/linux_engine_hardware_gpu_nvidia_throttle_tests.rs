use nvml_wrapper::bitmasks::device::ThrottleReasons;

use super::*;

#[test]
fn every_instantaneous_wrapper_bit_maps_once_in_stable_order() {
    let raw = (ThrottleReasons::GPU_IDLE
        | ThrottleReasons::APPLICATIONS_CLOCKS_SETTING
        | ThrottleReasons::SW_POWER_CAP
        | ThrottleReasons::HW_SLOWDOWN
        | ThrottleReasons::SYNC_BOOST
        | ThrottleReasons::SW_THERMAL_SLOWDOWN
        | ThrottleReasons::HW_THERMAL_SLOWDOWN
        | ThrottleReasons::HW_POWER_BRAKE_SLOWDOWN
        | ThrottleReasons::DISPLAY_CLOCK_SETTING)
        .bits();

    assert_eq!(
        map_throttle_bits(raw),
        vec![
            GpuThrottleReason::Idle,
            GpuThrottleReason::ApplicationClockLimit,
            GpuThrottleReason::SoftwarePowerLimit,
            GpuThrottleReason::HardwareSlowdown,
            GpuThrottleReason::SyncBoost,
            GpuThrottleReason::SoftwareThermalLimit,
            GpuThrottleReason::HardwareThermalLimit,
            GpuThrottleReason::ExternalPowerBrake,
            GpuThrottleReason::DisplayClockLimit,
        ]
    );
}

#[test]
fn known_and_future_bits_coexist_and_future_bits_collapse_to_one_other() {
    let unknown = (0..u64::BITS)
        .map(|shift| 1_u64 << shift)
        .find(|bit| ThrottleReasons::all().bits() & bit == 0)
        .expect("wrapper leaves at least one future bit");
    let second_unknown = (0..u64::BITS)
        .map(|shift| 1_u64 << shift)
        .find(|bit| *bit != unknown && ThrottleReasons::all().bits() & bit == 0)
        .expect("wrapper leaves a second future bit");

    assert_eq!(
        map_throttle_bits(ThrottleReasons::SW_POWER_CAP.bits() | unknown | second_unknown),
        vec![
            GpuThrottleReason::SoftwarePowerLimit,
            GpuThrottleReason::Other,
        ]
    );
}

#[test]
fn cumulative_reliability_policy_is_not_mislabeled_as_instantaneous() {
    let every_known = ThrottleReasons::all().bits();
    assert!(
        !map_throttle_bits(every_known).contains(&GpuThrottleReason::ReliabilityLimit),
        "nvml-wrapper 0.10 exposes reliability only as cumulative violation time"
    );
}
