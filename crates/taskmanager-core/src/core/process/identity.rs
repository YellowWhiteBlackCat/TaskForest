//! Validated live identity for process observations and renderer anchors.
//!
//! A PID is only a lookup hint: the operating system may reuse it after a
//! process exits. [`ProcessLiveKey`] pairs the PID with the provider-issued
//! start token and gives shared projection code an owned value that can
//! distinguish two incarnations of the same PID. It is deliberately weaker
//! than [`super::FrozenProcessIdentity`]: it identifies a live row, but does
//! not by itself authorize a control effect.

use std::num::{NonZeroU32, NonZeroU64};

use serde::{Deserialize, Serialize};

use super::{FrozenProcessIdentity, ProcessItem};
use crate::core::process_telemetry::ProcessIdentity;

/// Exact provider-issued identity of one live process observation.
///
/// The non-zero representation rejects the two values that cannot identify a
/// process. Providers must only compare keys produced by the same provider
/// family because the token's meaning is provider-native.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProcessLiveKey {
    pid: NonZeroU32,
    start_token: NonZeroU64,
}

impl ProcessLiveKey {
    /// Construct a key from a PID and its provider-issued start token.
    #[must_use]
    pub const fn new(pid: u32, start_token: u64) -> Option<Self> {
        let Some(pid) = NonZeroU32::new(pid) else {
            return None;
        };
        let Some(start_token) = NonZeroU64::new(start_token) else {
            return None;
        };
        Some(Self { pid, start_token })
    }

    /// Validate and copy a process-insights identity into the shared key.
    #[must_use]
    pub const fn from_identity(identity: ProcessIdentity) -> Option<Self> {
        Self::new(identity.pid, identity.start_token)
    }

    /// Validate the current identity of one process row.
    #[must_use]
    pub const fn from_process(process: &ProcessItem) -> Option<Self> {
        match process.current_start_token() {
            Some(start_token) => Self::new(process.pid, start_token),
            None => None,
        }
    }

    /// Construct from raw identity components at a projection boundary.
    #[must_use]
    pub const fn from_parts(pid: u32, start_token: u64) -> Option<Self> {
        Self::new(pid, start_token)
    }

    /// The PID as its ordinary platform-facing scalar.
    #[must_use]
    pub const fn pid(self) -> u32 {
        self.pid.get()
    }

    /// The provider-issued start token.
    #[must_use]
    pub const fn start_token(self) -> u64 {
        self.start_token.get()
    }

    /// Stable, provider-neutral key for row, semantic, and cache identities.
    ///
    /// The key intentionally contains the provider-issued start token. A PID
    /// by itself is only a lookup hint and may be reused by the operating
    /// system after the original process exits.
    #[must_use]
    pub fn stable_key(self) -> String {
        format!("pid:{}:start:{}", self.pid(), self.start_token())
    }

    /// Convert back to the provider-neutral process-insights identity shape.
    #[must_use]
    pub const fn into_identity(self) -> ProcessIdentity {
        ProcessIdentity {
            pid: self.pid(),
            start_token: self.start_token(),
        }
    }

    /// Borrow-free copy of the provider-neutral identity shape.
    #[must_use]
    pub const fn identity(self) -> ProcessIdentity {
        self.into_identity()
    }

    /// Exact equality against a raw provider identity.
    #[must_use]
    pub const fn matches(self, identity: ProcessIdentity) -> bool {
        self.pid() == identity.pid && self.start_token() == identity.start_token
    }
}

impl From<ProcessLiveKey> for ProcessIdentity {
    fn from(key: ProcessLiveKey) -> Self {
        key.into_identity()
    }
}

impl FrozenProcessIdentity {
    /// Recover the validated live lookup key from an exact frozen target.
    ///
    /// This is a bridge for reconciliation and diagnostics only. Control
    /// submission must continue to use the complete frozen identity so the
    /// provider can revalidate the target and the displayed name remains
    /// available for feedback.
    #[must_use]
    pub fn live_key(&self) -> Option<ProcessLiveKey> {
        ProcessLiveKey::new(self.pid, self.authoritative_start_token()?)
    }
}

impl ProcessItem {
    /// Return the current row's validated live identity, if its start token is
    /// currently available. Missing, stale, or unavailable tokens yield
    /// `None`; callers may still display the row, but must not treat it as an
    /// exact control target.
    #[must_use]
    pub const fn current_live_key(&self) -> Option<ProcessLiveKey> {
        match self.current_start_token() {
            Some(start_token) => ProcessLiveKey::new(self.pid, start_token),
            None => None,
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_process_identity_tests.rs"]
mod tests;
