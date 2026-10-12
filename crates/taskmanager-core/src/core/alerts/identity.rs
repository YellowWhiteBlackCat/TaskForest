//! Typed identities for alert rules, active instances, and event history.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

use crate::core::{DeviceGeneration, DeviceId};

/// Stable identity of one alert rule.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AlertRuleId(String);

impl AlertRuleId {
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

impl fmt::Display for AlertRuleId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl From<String> for AlertRuleId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for AlertRuleId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Non-zero sequence identity assigned when an alert event is appended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct AlertEventId(u64);

impl AlertEventId {
    #[must_use]
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for AlertEventId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl<'de> Deserialize<'de> for AlertEventId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u64::deserialize(deserializer)?;
        Self::new(value).ok_or_else(|| serde::de::Error::custom("alert event ID must be non-zero"))
    }
}

/// Typed target identity used to distinguish active alert instances.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AlertTargetIdentity {
    System,
    Storage {
        device_id: DeviceId,
        generation: DeviceGeneration,
    },
}

/// Composite identity of an active alert, without delimiter encoding.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AlertInstanceId {
    rule_id: AlertRuleId,
    target: AlertTargetIdentity,
}

impl AlertInstanceId {
    #[must_use]
    pub const fn new(rule_id: AlertRuleId, target: AlertTargetIdentity) -> Self {
        Self { rule_id, target }
    }

    #[must_use]
    pub const fn rule_id(&self) -> &AlertRuleId {
        &self.rule_id
    }

    #[must_use]
    pub const fn target(&self) -> &AlertTargetIdentity {
        &self.target
    }
}
