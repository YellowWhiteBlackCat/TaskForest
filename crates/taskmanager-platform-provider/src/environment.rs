use taskmanager_core::{
    SessionControlAction, SessionId, SessionItem, StartupBootEvidenceSnapshot, StartupEntry,
};
use taskmanager_platform_contract::{PartialSourceSnapshot, ProviderFailure};

pub trait StartupInventoryProvider: Send + 'static {
    fn refresh(&mut self) -> Result<PartialSourceSnapshot<StartupEntry>, ProviderFailure>;
}

pub trait StartupControlProvider: Send + 'static {
    fn set_enabled(&mut self, entry: &StartupEntry, enabled: bool) -> Result<(), ProviderFailure>;
}

pub trait StartupEvidenceProvider: Send + 'static {
    fn observe(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<StartupBootEvidenceSnapshot, ProviderFailure>;
}

pub trait SessionInventoryProvider: Send + 'static {
    fn refresh(&mut self) -> Result<PartialSourceSnapshot<SessionItem>, ProviderFailure>;
}

pub trait SessionControlProvider: Send + 'static {
    fn control(
        &mut self,
        session_id: &SessionId,
        action: SessionControlAction,
    ) -> Result<(), ProviderFailure>;
}
