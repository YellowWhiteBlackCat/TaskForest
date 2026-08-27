use std::path::Path;

use taskmanager_core::{
    AlertSeverity, DesktopAppearance, FrozenProcessIdentity, SetupScriptAction, SetupScriptEvent,
};
use taskmanager_platform_contract::{CompositeSourceSnapshot, ProviderFailure};

pub trait CommandLaunchProvider: Send + 'static {
    fn run_command(&mut self, command: &str) -> Result<u32, ProviderFailure>;
}

pub trait ResourceRevealProvider: Send + 'static {
    fn reveal_process(
        &mut self,
        target: &FrozenProcessIdentity,
        cached_executable: Option<&Path>,
    ) -> Result<(), ProviderFailure>;
}

pub trait UrlOpenProvider: Send + 'static {
    fn open_url(&mut self, url: &str) -> Result<(), ProviderFailure>;
}

pub trait DesktopAppearanceProvider: Send + 'static {
    fn observe(&mut self) -> Result<CompositeSourceSnapshot<DesktopAppearance>, ProviderFailure>;
}

/// Deliver a desktop notification for a fired alert (BN-07). Implementations
/// map to the platform notification service and classify failures typed
/// (no service -> `MissingDependency`, refused -> `TemporarilyUnavailable`).
/// Parameters are the decomposed request fields so the provider crate never
/// depends on `taskmanager-application` (dependency firewall).
pub trait DesktopNotificationProvider: Send + 'static {
    fn notify(
        &mut self,
        title: &str,
        body: &str,
        severity: AlertSeverity,
        target: &str,
    ) -> Result<(), ProviderFailure>;
}

/// Native first-run setup provider. Implementations own discovery of one
/// fixed setup asset and the corresponding auditable helper actions; they do
/// not accept arbitrary command strings.
pub trait SetupScriptProvider: Send + 'static {
    fn perform(&mut self, action: SetupScriptAction) -> Result<SetupScriptEvent, ProviderFailure>;
}
