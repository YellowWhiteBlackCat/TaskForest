//! Platform-neutral startup inventory contracts.

use serde::{Deserialize, Serialize};

use super::evidence::StartupImpactEvidence;
use super::identity::{StartupEntryId, StartupEntryLocator};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StartupSource {
    /// A desktop-session entry such as an XDG desktop file or equivalent.
    ///
    /// The legacy wire spelling is retained for snapshot compatibility; the
    /// Rust contract deliberately does not name a Linux directory standard.
    #[serde(
        rename = "XdgAutostart",
        alias = "DesktopEntry",
        alias = "desktop_entry"
    )]
    DesktopEntry,
    /// A service managed in the current user's startup scope.
    #[serde(rename = "SystemdUser", alias = "UserService", alias = "user_service")]
    UserService,
    /// A service managed in the machine-wide startup scope.
    SystemService,
    /// Membership in an ordered or named boot/run level.
    #[serde(rename = "OpenRcRunlevel", alias = "RunLevel", alias = "run_level")]
    RunLevel,
    /// A platform registry entry evaluated during login or boot.
    RegistryEntry,
    /// A task scheduled for login, boot, or session activation.
    ScheduledTask,
    /// A platform login-item mechanism.
    LoginItem,
    /// A filesystem-backed startup-folder entry.
    StartupFolder,
    /// A provider-native mechanism not represented by a baseline category.
    Other,
}

impl StartupSource {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DesktopEntry => "Desktop Entry",
            Self::UserService => "User Service",
            Self::SystemService => "System Service",
            Self::RunLevel => "Runlevel",
            Self::RegistryEntry => "Registry Entry",
            Self::ScheduledTask => "Scheduled Task",
            Self::LoginItem => "Login Item",
            Self::StartupFolder => "Startup Folder",
            Self::Other => "Other",
        }
    }
}

/// Native authority scope in which a startup entry is installed.
///
/// Scope is deliberately independent from the mechanism: desktop entries,
/// services, registry values and tasks may each exist at user or system scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StartupScope {
    User,
    System,
    Session,
    #[default]
    Unknown,
}

/// Control strategy the native provider can honestly offer for this row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StartupControlPolicy {
    /// The discovered native object can be changed in place after revalidation.
    Direct,
    /// The provider can create a user-scoped override without mutating the
    /// machine-wide source object.
    UserOverride,
    /// Inventory is available but mutation is not safely implemented.
    #[default]
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum StartupImpact {
    High,
    Medium,
    Low,
    #[default]
    None,
}

impl StartupImpact {
    /// Locale-catalog key for this tier (`startup.impact_high` …). The core
    /// layer stays copy-free: frontends resolve the key through their own
    /// i18n facade, so no English UI text leaks from the domain model.
    #[must_use]
    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::High => "startup.impact_high",
            Self::Medium => "startup.impact_medium",
            Self::Low => "startup.impact_low",
            Self::None => "startup.impact_none",
        }
    }

    #[must_use]
    pub fn from_millis(milliseconds: u64) -> Self {
        if milliseconds > 500 {
            Self::High
        } else if milliseconds > 100 {
            Self::Medium
        } else {
            Self::Low
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StartupEntry {
    #[serde(default)]
    pub id: StartupEntryId,
    pub name: String,
    pub exec: String,
    pub enabled: bool,
    pub source: StartupSource,
    #[serde(default)]
    pub scope: StartupScope,
    #[serde(default)]
    pub control_policy: StartupControlPolicy,
    #[serde(rename = "handle", alias = "locator")]
    pub locator: StartupEntryLocator,
    pub impact: StartupImpact,
    pub impact_evidence: StartupImpactEvidence,
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_startup_inventory_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/headless/core_core_startup_inventory_impact_tests.rs"]
mod impact_tests;
