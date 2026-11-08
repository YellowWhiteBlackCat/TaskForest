//! Provider-neutral service lifecycle actions.

use serde::{Deserialize, Serialize};

/// A lifecycle action understood by every native service provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceAction {
    Start,
    Stop,
    Restart,
    Enable,
    Disable,
    /// Ask the selected systemd manager to rescan unit files. The provider
    /// ignores the selected unit for the native command because this is a
    /// manager-wide `daemon-reload`, not a per-service lifecycle transition.
    ReloadDaemon,
}

impl ServiceAction {
    /// Stable transport token for this action, shared by target keys and
    /// provider command vocabularies. It is not display copy.
    #[must_use]
    pub const fn stable_token(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
            Self::Enable => "enable",
            Self::Disable => "disable",
            Self::ReloadDaemon => "reload_daemon",
        }
    }

    /// The one catalog key for the action word. Frontends resolve it through
    /// the shared i18n owner; they must not fork this match.
    #[must_use]
    pub const fn i18n_key(self) -> &'static str {
        match self {
            Self::Start => "svc.start",
            Self::Stop => "svc.stop",
            Self::Restart => "svc.restart",
            Self::Enable => "svc.enable",
            Self::Disable => "svc.disable",
            Self::ReloadDaemon => "svc.reload_daemon",
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_service_action_vocabulary_tests.rs"]
mod tests;
