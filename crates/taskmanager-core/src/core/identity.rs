//! Platform-neutral provider and device identity primitives.

use std::borrow::{Borrow, Cow};
use std::fmt;

use serde::{Deserialize, Serialize};

/// Stable identifier for one native provider implementation.
///
/// Provider IDs describe implementations such as `linux.procfs` or
/// `linux.nvml`; they are diagnostics metadata, not product or hardware SKUs.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderId(Cow<'static, str>);

impl ProviderId {
    #[must_use]
    pub const fn borrowed(value: &'static str) -> Self {
        Self(Cow::Borrowed(value))
    }

    #[must_use]
    pub fn owned(value: impl Into<String>) -> Self {
        Self(Cow::Owned(value.into()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Stable identity of a physical, logical, or virtual device.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeviceId(String);

impl DeviceId {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl AsRef<str> for DeviceId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for DeviceId {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl From<String> for DeviceId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for DeviceId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Generation of a stable device identity.
///
/// It advances only after a confirmed absent-to-present transition. Provider
/// failure alone is never evidence of a new generation.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct DeviceGeneration(u64);

impl DeviceGeneration {
    pub const INITIAL: Self = Self(1);

    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

#[cfg(test)]
#[path = "../../tests/headless/core_core_identity_tests.rs"]
mod tests;
