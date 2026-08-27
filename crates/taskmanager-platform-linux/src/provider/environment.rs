//! Linux startup-entry and session providers.

use taskmanager_core::{
    SessionControlAction, SessionId, SessionItem, StartupBootEvidenceSnapshot, StartupEntry,
};
use taskmanager_platform_contract::{
    PartialSourceSnapshot, ProviderFailure, ProviderId, SourceOutcome, SourceStatus,
};
use taskmanager_platform_provider::{
    SessionControlProvider, SessionInventoryProvider, StartupControlProvider,
    StartupEvidenceProvider, StartupInventoryProvider,
};

use crate::engine::session::{SessionManager, SessionScanFailure};
use crate::engine::startup::StartupManager;

pub(super) struct NativeStartupProvider {
    pub(super) manager: StartupManager,
}

impl StartupInventoryProvider for NativeStartupProvider {
    fn refresh(&mut self) -> Result<PartialSourceSnapshot<StartupEntry>, ProviderFailure> {
        #[cfg(target_os = "linux")]
        {
            Ok(self.manager.scan_snapshot())
        }
        #[cfg(not(target_os = "linux"))]
        {
            Err(ProviderFailure::TemporarilyUnavailable)
        }
    }
}

impl StartupControlProvider for NativeStartupProvider {
    fn set_enabled(&mut self, entry: &StartupEntry, enabled: bool) -> Result<(), ProviderFailure> {
        #[cfg(target_os = "linux")]
        {
            self.manager.set_enabled(entry, enabled)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (entry, enabled);
            Err(ProviderFailure::TemporarilyUnavailable)
        }
    }
}

pub(super) struct NativeStartupEvidenceProvider;

impl StartupEvidenceProvider for NativeStartupEvidenceProvider {
    fn observe(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<StartupBootEvidenceSnapshot, ProviderFailure> {
        Ok(crate::engine::startup::evidence::collect_startup_boot_evidence(observed_at_ms))
    }
}

pub(super) struct NativeSessionProvider {
    pub(super) manager: SessionManager,
}

impl SessionInventoryProvider for NativeSessionProvider {
    fn refresh(&mut self) -> Result<PartialSourceSnapshot<SessionItem>, ProviderFailure> {
        #[cfg(target_os = "linux")]
        {
            self.manager
                .try_scan()
                .map(|sessions| {
                    let item_count = sessions.len();
                    let outcome = if sessions.is_empty() {
                        SourceOutcome::Empty
                    } else {
                        SourceOutcome::Available
                    };
                    PartialSourceSnapshot::new(
                        sessions,
                        vec![SourceStatus {
                            provider: ProviderId::borrowed("linux.session.logind"),
                            outcome,
                            item_count,
                        }],
                    )
                })
                .map_err(|failure| match failure {
                    SessionScanFailure::Unavailable => ProviderFailure::TemporarilyUnavailable,
                    SessionScanFailure::PermissionDenied => ProviderFailure::PermissionDenied,
                    SessionScanFailure::TimedOut => ProviderFailure::TimedOut,
                    SessionScanFailure::ProviderFailed => ProviderFailure::ProviderFault,
                })
        }
        #[cfg(not(target_os = "linux"))]
        {
            Err(ProviderFailure::TemporarilyUnavailable)
        }
    }
}

impl SessionControlProvider for NativeSessionProvider {
    fn control(
        &mut self,
        session_id: &SessionId,
        action: SessionControlAction,
    ) -> Result<(), ProviderFailure> {
        #[cfg(target_os = "linux")]
        {
            let result = match action {
                SessionControlAction::Disconnect => {
                    self.manager.terminate_session(session_id.as_str())
                }
                SessionControlAction::Lock => self.manager.lock_session(session_id.as_str()),
            };
            result.map_err(|error| classify_session_control_error(&error))
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (session_id, action);
            Err(ProviderFailure::TemporarilyUnavailable)
        }
    }
}

fn classify_session_control_error(error: &str) -> ProviderFailure {
    let lower = error.to_ascii_lowercase();
    if lower.contains("permission denied") || lower.contains("access denied") {
        ProviderFailure::PermissionDenied
    } else if lower.contains("timed out") || lower.contains("timeout") {
        ProviderFailure::TimedOut
    } else if lower.contains("no session") || lower.contains("unknown session") {
        ProviderFailure::IdentityChanged
    } else if lower.contains("no such file")
        || lower.contains("not supported")
        || lower.contains("unsupported")
    {
        ProviderFailure::TemporarilyUnavailable
    } else {
        ProviderFailure::Rejected
    }
}

#[cfg(test)]
#[path = "../../tests/headless/linux_provider_environment_tests.rs"]
mod tests;
