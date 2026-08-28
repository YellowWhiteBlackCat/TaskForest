//! Windows WTS session inventory and session-control providers.

use taskmanager_core::{SessionControlAction, SessionId, SessionItem};
use taskmanager_platform_contract::{
    PartialSourceSnapshot, ProviderFailure, SourceOutcome, SourceStatus,
};
use taskmanager_platform_provider::{SessionControlProvider, SessionInventoryProvider};
use taskmanager_windows_api::{enumerate_sessions, lock_workstation, logoff_session};

use super::{
    SESSION_INVENTORY_PROVIDER, WinSessionControlProvider, WinSessionInventoryProvider,
    map_windows_api_failure,
};

impl SessionInventoryProvider for WinSessionInventoryProvider {
    fn refresh(&mut self) -> Result<PartialSourceSnapshot<SessionItem>, ProviderFailure> {
        let sessions = enumerate_sessions().map_err(map_windows_api_failure)?;
        let items = sessions
            .into_iter()
            .map(|session| {
                let session_name = session.session_name.unwrap_or_default();
                let remote = session_name.to_ascii_lowercase().starts_with("rdp");
                SessionItem {
                    id: format!("windows:session:{}", session.session_id),
                    uid: 0,
                    user: session.user_name.unwrap_or_default(),
                    seat: None,
                    tty: (!session_name.is_empty()).then_some(session_name),
                    remote,
                    timestamp: None,
                }
            })
            .collect::<Vec<_>>();
        let item_count = items.len();
        Ok(PartialSourceSnapshot::new(
            items,
            vec![SourceStatus {
                provider: SESSION_INVENTORY_PROVIDER,
                outcome: SourceOutcome::Available,
                item_count,
            }],
        ))
    }
}
impl SessionControlProvider for WinSessionControlProvider {
    fn control(
        &mut self,
        session_id: &SessionId,
        action: SessionControlAction,
    ) -> Result<(), ProviderFailure> {
        match action {
            SessionControlAction::Lock => lock_workstation().map_err(map_windows_api_failure),
            SessionControlAction::Disconnect => {
                let raw = session_id
                    .as_str()
                    .strip_prefix("windows:session:")
                    .ok_or(ProviderFailure::IdentityChanged)?;
                let session_id = raw
                    .parse::<u32>()
                    .ok()
                    .filter(|session_id| *session_id > 0)
                    .ok_or(ProviderFailure::IdentityChanged)?;
                logoff_session(session_id).map_err(map_windows_api_failure)
            }
        }
    }
}
