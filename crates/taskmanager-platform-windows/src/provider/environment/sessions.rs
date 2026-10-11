//! Windows WTS session inventory and session-control providers.

use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::session::{
    SessionControlAction, SessionIdentity, SessionIncarnation, SessionItem,
};
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_platform_contract::{PartialSourceSnapshot, ProviderFailure};
use taskmanager_platform_provider::{SessionControlProvider, SessionInventoryProvider};
use taskmanager_windows_api::{
    WindowsApiError, WindowsSession, WindowsSessionIdentity, WindowsSessionIncarnation,
    enumerate_sessions, lock_current_session, logoff_session,
};

use super::{
    SESSION_INVENTORY_PROVIDER, WinSessionControlProvider, WinSessionInventoryProvider,
    map_windows_api_failure,
};

impl SessionInventoryProvider for WinSessionInventoryProvider {
    fn refresh(&mut self) -> Result<PartialSourceSnapshot<SessionItem>, ProviderFailure> {
        let sessions = enumerate_sessions().map_err(map_windows_api_failure)?;
        Ok(project_windows_session_snapshot(sessions))
    }
}

pub(super) fn project_windows_session_snapshot(
    sessions: Vec<WindowsSession>,
) -> PartialSourceSnapshot<SessionItem> {
    let mut identity_failure = None;
    let items = sessions
        .into_iter()
        .map(|session| {
            let session_name = session.session_name.unwrap_or_default();
            let remote = session_name.to_ascii_lowercase().starts_with("rdp");
            let incarnation = match session.incarnation {
                Ok(incarnation) => Some(SessionIncarnation::WindowsWts {
                    logon_time_100ns: incarnation.logon_time_100ns(),
                }),
                Err(error) => {
                    identity_failure.get_or_insert(session_identity_failure(error));
                    None
                }
            };
            SessionItem {
                id: format!("windows:session:{}", session.session_id).into(),
                incarnation,
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
    let outcome = identity_failure.map_or(SourceOutcome::Available, SourceOutcome::Partial);
    PartialSourceSnapshot::new(
        items,
        vec![SourceStatus {
            provider: SESSION_INVENTORY_PROVIDER,
            outcome,
            item_count,
        }],
    )
}
impl SessionControlProvider for WinSessionControlProvider {
    fn control(
        &mut self,
        identity: &SessionIdentity,
        action: SessionControlAction,
    ) -> Result<(), ProviderFailure> {
        let target = windows_session_identity(identity)?;
        match action {
            SessionControlAction::Lock => {
                lock_current_session(target).map_err(map_session_control_failure)
            }
            SessionControlAction::Disconnect => {
                logoff_session(target).map_err(map_session_control_failure)
            }
        }
    }
}

fn windows_session_identity(
    identity: &SessionIdentity,
) -> Result<WindowsSessionIdentity, ProviderFailure> {
    let raw = identity
        .session_id
        .as_str()
        .strip_prefix("windows:session:")
        .ok_or(ProviderFailure::IdentityChanged)?;
    let session_id = raw
        .parse::<u32>()
        .ok()
        .filter(|session_id| *session_id > 0)
        .ok_or(ProviderFailure::IdentityChanged)?;
    let SessionIncarnation::WindowsWts { logon_time_100ns } = &identity.incarnation else {
        return Err(ProviderFailure::IdentityChanged);
    };
    let incarnation = WindowsSessionIncarnation::new(*logon_time_100ns)
        .ok_or(ProviderFailure::IdentityChanged)?;
    WindowsSessionIdentity::new(session_id, incarnation).ok_or(ProviderFailure::IdentityChanged)
}

fn session_identity_failure(error: WindowsApiError) -> FailureKind {
    match error {
        WindowsApiError::Unsupported => FailureKind::Unsupported,
        WindowsApiError::PermissionDenied => FailureKind::PermissionDenied,
        WindowsApiError::IdentityChanged | WindowsApiError::InvalidInput => {
            FailureKind::IdentityChanged
        }
        WindowsApiError::InvalidText
        | WindowsApiError::ResourceLimit
        | WindowsApiError::QueryFailed => FailureKind::TemporarilyUnavailable,
    }
}

fn map_session_control_failure(error: WindowsApiError) -> ProviderFailure {
    match error {
        WindowsApiError::Unsupported if cfg!(windows) => ProviderFailure::Unsupported,
        WindowsApiError::Unsupported => ProviderFailure::MissingDependency,
        WindowsApiError::PermissionDenied => ProviderFailure::PermissionDenied,
        WindowsApiError::IdentityChanged | WindowsApiError::InvalidInput => {
            ProviderFailure::IdentityChanged
        }
        WindowsApiError::InvalidText
        | WindowsApiError::ResourceLimit
        | WindowsApiError::QueryFailed => ProviderFailure::TemporarilyUnavailable,
    }
}
