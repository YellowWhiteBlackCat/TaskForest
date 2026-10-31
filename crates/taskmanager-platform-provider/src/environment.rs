use taskforest_contract::{PartialSourceSnapshot, ProviderFailure};
use taskforest_core::{
    SessionControlAction, SessionIdentity, SessionItem, StartupBootEvidenceSnapshot, StartupEntry,
};

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
        identity: &SessionIdentity,
        action: SessionControlAction,
    ) -> Result<(), ProviderFailure>;
}
