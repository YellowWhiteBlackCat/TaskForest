//! Platform-neutral aggregate health observations.
//!
//! Discovery and command execution stay in platform providers. These models
//! carry only stable device identity, presentation metadata, and typed health
//! observations so every frontend can consume the same contract.

use serde::{Deserialize, Serialize};

use crate::core::{
    DeviceGeneration, DeviceId, FilesystemHealthSnapshot, SensorCenterSnapshot, SmartSelfTestKind,
    SmartSelfTestReport, StorageDeviceKey, StorageDeviceTarget,
};

/// Provider-neutral identity and intent for a destructive-capable SMART job.
///
/// `device_id`/`device_generation` identify the physical target. `device_key`
/// stays an opaque command locator: a Linux provider may resolve it to a
/// block-device name while another platform can use its native locator.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmartSelfTestIntent {
    /// Stable physical disk identity selected from the telemetry lifecycle.
    #[serde(default)]
    pub device_id: DeviceId,
    /// Disk generation at confirmation time; never a SMART-job generation.
    #[serde(default)]
    pub device_generation: DeviceGeneration,
    /// Provider-native locator used only to address the command.
    pub device_key: StorageDeviceKey,
    pub display_name: String,
    pub kind: SmartSelfTestKind,
}

impl SmartSelfTestIntent {
    /// Return the complete mutation target supplied to a native provider.
    ///
    /// Keeping identity, lifecycle generation, and native locator together
    /// allows the adapter to reject a stale locator after hot-plug.
    #[must_use]
    pub fn target(&self) -> StorageDeviceTarget {
        StorageDeviceTarget {
            device_id: self.device_id.clone(),
            device_generation: self.device_generation,
            locator: self.device_key.clone(),
        }
    }

    #[must_use]
    pub fn into_observation(self, report: SmartSelfTestReport) -> SmartSelfTestObservation {
        SmartSelfTestObservation {
            device_id: self.device_id,
            device_generation: self.device_generation,
            device_key: self.device_key,
            display_name: self.display_name,
            report,
        }
    }
}

/// Selected device plus the newest typed self-test report.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SmartSelfTestObservation {
    /// Stable physical target identity retained across native locator changes.
    #[serde(default)]
    pub device_id: DeviceId,
    /// Physical target generation copied from the confirmed intent.
    #[serde(default)]
    pub device_generation: DeviceGeneration,
    /// Provider-native locator; this is not lifecycle identity.
    pub device_key: StorageDeviceKey,
    pub display_name: String,
    pub report: SmartSelfTestReport,
}

impl SmartSelfTestObservation {
    /// Return the same generation-bound target used to start the job.
    #[must_use]
    pub fn target(&self) -> StorageDeviceTarget {
        StorageDeviceTarget {
            device_id: self.device_id.clone(),
            device_generation: self.device_generation,
            locator: self.device_key.clone(),
        }
    }
}

/// Latest cross-provider health state.
///
/// Event ordering and correlation live in the platform contract envelope, so
/// this domain snapshot deliberately carries no transport sequence number.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SystemHealthSnapshot {
    pub filesystems: FilesystemHealthSnapshot,
    pub sensors: SensorCenterSnapshot,
    /// Independently tracked jobs keyed by physical device lifecycle identity.
    /// Presentation layers may select one row without collapsing this model.
    pub self_tests: Vec<SmartSelfTestObservation>,
}

/// Health dimensions that can produce a transparent score deduction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthDeductionKind {
    CpuPressure,
    MemoryPressure,
    MemoryFullStall,
    IoPressure,
    SwapPressure,
    ThermalThrottle,
}

/// One auditable score deduction. `evidence` carries the exact observed value
/// that caused the deduction; there is no opaque magic penalty.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthDeduction {
    pub kind: HealthDeductionKind,
    pub points: u8,
    pub evidence: String,
}

/// Inputs accepted by the pure health-score calculator. Missing inputs simply
/// do not create a deduction; they never become a healthy zero.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HealthScoreInput {
    pub cpu_some_avg10: Option<f32>,
    pub memory_some_avg10: Option<f32>,
    pub memory_full_avg10: Option<f32>,
    pub io_some_avg10: Option<f32>,
    pub swap_used_pct: Option<f32>,
    pub thermal_throttled: Option<bool>,
}

/// Transparent system health score with bounded deductions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemHealthScore {
    pub score: u8,
    pub deductions: Vec<HealthDeduction>,
}

impl SystemHealthScore {
    /// Calculate a score from observed evidence. `None` means no input was
    /// observed at all, so the UI can show "unknown" rather than 100.
    #[must_use]
    pub fn calculate(input: HealthScoreInput) -> Option<Self> {
        let any_observed = [
            input.cpu_some_avg10,
            input.memory_some_avg10,
            input.memory_full_avg10,
            input.io_some_avg10,
            input.swap_used_pct,
            input.thermal_throttled.map(|_| 0.0),
        ]
        .into_iter()
        .flatten()
        .any(|value| value.is_finite());
        if !any_observed {
            return None;
        }
        let mut deductions = Vec::new();
        add_pressure_deduction(
            &mut deductions,
            HealthDeductionKind::CpuPressure,
            input.cpu_some_avg10,
            10.0,
            25,
        );
        add_pressure_deduction(
            &mut deductions,
            HealthDeductionKind::MemoryPressure,
            input.memory_some_avg10,
            10.0,
            20,
        );
        add_pressure_deduction(
            &mut deductions,
            HealthDeductionKind::MemoryFullStall,
            input.memory_full_avg10,
            1.0,
            35,
        );
        add_pressure_deduction(
            &mut deductions,
            HealthDeductionKind::IoPressure,
            input.io_some_avg10,
            10.0,
            20,
        );
        if let Some(used) = input.swap_used_pct.filter(|value| value.is_finite())
            && used >= 80.0
        {
            deductions.push(HealthDeduction {
                kind: HealthDeductionKind::SwapPressure,
                points: (((used - 80.0) / 20.0 * 20.0).round() as u8).clamp(1, 20),
                evidence: format!("swap used {used:.1}%"),
            });
        }
        if input.thermal_throttled == Some(true) {
            deductions.push(HealthDeduction {
                kind: HealthDeductionKind::ThermalThrottle,
                points: 20,
                evidence: "thermal throttle asserted".to_owned(),
            });
        }
        let deducted = deductions
            .iter()
            .map(|deduction| u16::from(deduction.points))
            .sum::<u16>();
        Some(Self {
            score: 100_u16.saturating_sub(deducted).min(100) as u8,
            deductions,
        })
    }

    /// Whether any observed deduction is present.
    #[must_use]
    pub fn is_degraded(&self) -> bool {
        !self.deductions.is_empty()
    }
}

fn add_pressure_deduction(
    deductions: &mut Vec<HealthDeduction>,
    kind: HealthDeductionKind,
    value: Option<f32>,
    threshold: f32,
    max_points: u8,
) {
    let Some(value) = value.filter(|value| value.is_finite() && *value > threshold) else {
        return;
    };
    let points = (((value - threshold) / (100.0 - threshold)) * f32::from(max_points))
        .round()
        .clamp(1.0, f32::from(max_points)) as u8;
    deductions.push(HealthDeduction {
        kind,
        points,
        evidence: format!("pressure avg10 {value:.1}%"),
    });
}

#[cfg(test)]
#[path = "../../tests/headless/core_core_system_health_tests.rs"]
mod tests;
