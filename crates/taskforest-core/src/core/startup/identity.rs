//! Opaque identities and provider-native locators for startup entries.

use serde::{Deserialize, Serialize};

/// Opaque provider-native address for one startup entry.
///
/// A locator may be a file path, service name, registry address, scheduled-task
/// key, or another native token. It is never a display name or cross-platform
/// identity, and shared callers must not parse its contents.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StartupEntryLocator(String);

impl StartupEntryLocator {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for StartupEntryLocator {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for StartupEntryLocator {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Opaque provider-issued identity of one logical startup entry.
///
/// Unlike a display name, this value remains unambiguous across providers and
/// scopes. Shared callers compare it for selection/correlation but never parse
/// it or derive a native command target from it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StartupEntryId(String);

impl StartupEntryId {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<String> for StartupEntryId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for StartupEntryId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_startup_identity_startup_entry_id_tests.rs"]
mod startup_entry_id_tests;
