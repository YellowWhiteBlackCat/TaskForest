//! Shared deterministic failed-unit and boot-chain observations.

use taskmanager_core::core::device_state::DeviceState;
use taskmanager_core::core::startup::{
    StartupBootEvidenceSnapshot, StartupCriticalChainNode, StartupFailedUnit,
};

#[must_use]
pub fn startup_failure_evidence(timestamp_ms: u64) -> StartupBootEvidenceSnapshot {
    let healthy = DeviceState::healthy(timestamp_ms);
    StartupBootEvidenceSnapshot {
        state: healthy,
        failed_units_state: healthy,
        critical_chain_state: healthy,
        failed_units_failure: None,
        critical_chain_failure: None,
        failed_units: [
            "taskforest-g.service",
            "taskforest-i.service",
            "taskforest.service",
        ]
        .into_iter()
        .map(|unit| StartupFailedUnit {
            unit: unit.into(),
            load_state: "loaded".into(),
            active_state: "failed".into(),
            sub_state: "failed".into(),
            description: "capture fixture failed unit".into(),
        })
        .collect(),
        critical_chain: vec![
            StartupCriticalChainNode {
                unit: "dbus.socket".into(),
                activated_at_ms: Some(0),
                duration_ms: Some(6),
            },
            StartupCriticalChainNode {
                unit: "graphical-session.target".into(),
                activated_at_ms: Some(6),
                duration_ms: Some(0),
            },
        ],
    }
}
