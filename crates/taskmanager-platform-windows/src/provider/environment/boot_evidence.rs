//! Windows startup boot-evidence provider and Diagnostics-Performance mapping.

#[cfg(windows)]
use taskmanager_core::StartupCriticalChainNode;
use taskmanager_core::{
    DeviceState, DeviceStatus, StartupBootEvidenceSnapshot, StartupEvidenceFailure,
};
use taskmanager_platform_contract::ProviderFailure;
use taskmanager_platform_provider::StartupEvidenceProvider;
#[cfg(windows)]
use taskmanager_windows_api::{
    WindowsApiError, WindowsEventLogEntry, WindowsEventLogQuery, query_event_log,
};

use super::WinStartupEvidenceProvider;
#[cfg(windows)]
use super::{BOOT_EVIDENCE_BOOT_EVENT_ID, BOOT_EVIDENCE_CHANNEL};

impl StartupEvidenceProvider for WinStartupEvidenceProvider {
    fn observe(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<StartupBootEvidenceSnapshot, ProviderFailure> {
        #[cfg(windows)]
        {
            Ok(collect_boot_evidence_windows(observed_at_ms))
        }
        #[cfg(not(windows))]
        {
            // The Diagnostics-Performance channel exists only on Windows; the
            // snapshot stays a typed Unsupported state, never an empty
            // success borrowed from a dormant source.
            let state =
                DeviceState::default().transition(DeviceStatus::Unsupported, observed_at_ms);
            Ok(StartupBootEvidenceSnapshot {
                state,
                failed_units_state: state,
                critical_chain_state: state,
                failed_units_failure: Some(StartupEvidenceFailure::Unsupported),
                critical_chain_failure: Some(StartupEvidenceFailure::Unsupported),
                failed_units: Vec::new(),
                critical_chain: Vec::new(),
            })
        }
    }
}

#[cfg(windows)]
fn collect_boot_evidence_windows(now_ms: u64) -> StartupBootEvidenceSnapshot {
    let query = WindowsEventLogQuery {
        channel: BOOT_EVIDENCE_CHANNEL.to_string(),
        provider: None,
        event_id: Some(BOOT_EVIDENCE_BOOT_EVENT_ID),
        after_record_id: None,
    };
    match query_event_log(&query, 1) {
        Ok(entries) => {
            // A readable channel with no recorded boot event is an honest
            // empty chain, not a failure.
            let critical_chain = entries
                .last()
                .map(|entry| {
                    vec![StartupCriticalChainNode {
                        unit: "Windows Boot".to_string(),
                        activated_at_ms: None,
                        duration_ms: boot_duration_ms(entry),
                    }]
                })
                .unwrap_or_default();
            let healthy = DeviceState::healthy(now_ms);
            StartupBootEvidenceSnapshot {
                state: healthy,
                failed_units_state: healthy,
                critical_chain_state: healthy,
                failed_units_failure: None,
                critical_chain_failure: None,
                failed_units: Vec::new(),
                critical_chain,
            }
        }
        Err(WindowsApiError::PermissionDenied) => boot_evidence_degraded(
            now_ms,
            StartupEvidenceFailure::PermissionDenied,
            DeviceStatus::PermissionDenied,
        ),
        Err(_) => boot_evidence_degraded(
            now_ms,
            StartupEvidenceFailure::Unavailable,
            DeviceStatus::Stale,
        ),
    }
}

#[cfg(windows)]
fn boot_evidence_degraded(
    now_ms: u64,
    failure: StartupEvidenceFailure,
    status: DeviceStatus,
) -> StartupBootEvidenceSnapshot {
    let state = DeviceState::default().transition(status, now_ms);
    StartupBootEvidenceSnapshot {
        state,
        failed_units_state: state,
        critical_chain_state: state,
        failed_units_failure: Some(failure),
        critical_chain_failure: Some(failure),
        failed_units: Vec::new(),
        critical_chain: Vec::new(),
    }
}

#[cfg(windows)]
fn boot_duration_ms(entry: &WindowsEventLogEntry) -> Option<u64> {
    // Event 100's documented event data carries the measured durations in
    // milliseconds; the main-path time is the boot proper, the total boot
    // time is the documented fallback.
    entry
        .properties
        .iter()
        .find_map(|(name, value)| match name.as_str() {
            "MainPathBootTime" | "BootTime" => value.parse().ok(),
            _ => None,
        })
}
