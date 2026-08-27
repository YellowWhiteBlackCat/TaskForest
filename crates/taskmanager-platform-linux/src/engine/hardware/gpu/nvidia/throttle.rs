//! Provider-neutral mapping for NVML's instantaneous clock-throttle mask.

use taskmanager_core::GpuThrottleReason;

use crate::engine::nvml::{NvmlFailureKind, classify_error};

pub(super) fn read_throttle_reasons(
    device: &nvml_wrapper::Device<'_>,
) -> Result<Vec<GpuThrottleReason>, NvmlFailureKind> {
    use nvml_wrapper::error::{Bits, NvmlError};

    match device.current_throttle_reasons_strict() {
        Ok(reasons) => Ok(map_throttle_bits(reasons.bits())),
        Err(NvmlError::IncorrectBits(Bits::U64(raw))) => Ok(map_throttle_bits(raw)),
        Err(error) => Err(classify_error(&error)),
    }
}

fn map_throttle_bits(raw: u64) -> Vec<GpuThrottleReason> {
    use nvml_wrapper::bitmasks::device::ThrottleReasons;

    let reasons = ThrottleReasons::from_bits_truncate(raw);
    let mut mapped = [
        (ThrottleReasons::GPU_IDLE, GpuThrottleReason::Idle),
        (
            ThrottleReasons::APPLICATIONS_CLOCKS_SETTING,
            GpuThrottleReason::ApplicationClockLimit,
        ),
        (
            ThrottleReasons::SW_POWER_CAP,
            GpuThrottleReason::SoftwarePowerLimit,
        ),
        (
            ThrottleReasons::HW_SLOWDOWN,
            GpuThrottleReason::HardwareSlowdown,
        ),
        (ThrottleReasons::SYNC_BOOST, GpuThrottleReason::SyncBoost),
        (
            ThrottleReasons::SW_THERMAL_SLOWDOWN,
            GpuThrottleReason::SoftwareThermalLimit,
        ),
        (
            ThrottleReasons::HW_THERMAL_SLOWDOWN,
            GpuThrottleReason::HardwareThermalLimit,
        ),
        (
            ThrottleReasons::HW_POWER_BRAKE_SLOWDOWN,
            GpuThrottleReason::ExternalPowerBrake,
        ),
        (
            ThrottleReasons::DISPLAY_CLOCK_SETTING,
            GpuThrottleReason::DisplayClockLimit,
        ),
    ]
    .into_iter()
    .filter_map(|(flag, reason)| reasons.contains(flag).then_some(reason))
    .collect::<Vec<_>>();
    if raw & !ThrottleReasons::all().bits() != 0 {
        mapped.push(GpuThrottleReason::Other);
    }
    mapped
}

#[cfg(test)]
#[path = "../../../../../tests/headless/linux_engine_hardware_gpu_nvidia_throttle_tests.rs"]
mod tests;
