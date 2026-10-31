//! Subscription mask representing on-demand telemetry facets requested by clients.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Bitmask of telemetry facets that a client may subscribe to.
///
/// When no client subscribes to high-overhead facets (e.g. per-socket network
/// or deep process trees), the background daemon downscales or skips their sampling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct SubscriptionMask(u32);

impl SubscriptionMask {
    pub const NONE: Self = Self(0);
    pub const CPU: Self = Self(1 << 0);
    pub const MEMORY: Self = Self(1 << 1);
    pub const DISK: Self = Self(1 << 2);
    pub const NETWORK: Self = Self(1 << 3);
    pub const GPU: Self = Self(1 << 4);
    pub const SENSORS: Self = Self(1 << 5);
    pub const PROCESSES: Self = Self(1 << 6);
    pub const SYSTEM_HEALTH: Self = Self(1 << 7);
    pub const BATTERY: Self = Self(1 << 8);
    pub const NPU: Self = Self(1 << 9);

    /// Combined bitmask representing all standard hardware telemetry.
    pub const HARDWARE_ALL: Self = Self(
        Self::CPU.0
            | Self::MEMORY.0
            | Self::DISK.0
            | Self::NETWORK.0
            | Self::GPU.0
            | Self::SENSORS.0
            | Self::BATTERY.0
            | Self::NPU.0,
    );

    /// Full subscription mask including processes and system health.
    pub const ALL: Self = Self(Self::HARDWARE_ALL.0 | Self::PROCESSES.0 | Self::SYSTEM_HEALTH.0);

    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    #[must_use]
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    #[must_use]
    pub const fn bits(&self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn contains(&self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }

    pub fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[must_use]
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0 == 0
    }
}

impl std::ops::BitOr for SubscriptionMask {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self::Output {
        self.union(rhs)
    }
}

impl std::ops::BitAnd for SubscriptionMask {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self::Output {
        self.intersection(rhs)
    }
}

impl fmt::Display for SubscriptionMask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SubscriptionMask(0x{:04x})", self.0)
    }
}
