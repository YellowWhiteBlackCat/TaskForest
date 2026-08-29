//! Request correlation identifiers, monotonic event sequence, and the request
//! and event envelopes that pair a payload with its capability-facet outcome.
//!
//! Successful payloads carry the envelope as their only correlation authority;
//! `EventEnvelope` validates the duplicated metadata copy on a provider failure.

use crate::{CapabilityId, OperationFailure};
use taskmanager_core::ProviderId;

/// Non-zero correlation identifier allocated by the application layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RequestId(u64);

impl RequestId {
    /// Smallest valid request identity, used by deterministic demo fixtures.
    pub const MIN: Self = Self(1);

    #[must_use]
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Monotonic request allocator. Wrapping skips zero.
#[derive(Clone, Debug, Default)]
pub struct RequestIdGenerator {
    next: u64,
}

impl RequestIdGenerator {
    #[must_use]
    pub fn next_id(&mut self) -> RequestId {
        self.next = self.next.wrapping_add(1);
        if self.next == 0 {
            self.next = 1;
        }
        RequestId(self.next)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EventSequence(u64);

impl EventSequence {
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

/// Correlated request submitted to one capability facet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestEnvelope<T> {
    pub id: RequestId,
    pub capability: CapabilityId,
    pub submitted_at_ms: u64,
    pub payload: T,
}

/// Correlated event produced after a request is accepted.
#[derive(Clone, Debug, PartialEq)]
pub struct EventEnvelope<T> {
    pub request_id: RequestId,
    pub capability: CapabilityId,
    pub provider: Option<ProviderId>,
    pub sequence: EventSequence,
    pub observed_at_ms: u64,
    pub outcome: Result<T, OperationFailure>,
}

impl<T> EventEnvelope<T> {
    /// Validate the duplicated correlation metadata on a provider failure.
    ///
    /// Successful payloads carry the envelope as their only correlation
    /// authority. Failure payloads historically carried a second copy so the
    /// application can expose a typed operation error; mismatches must never
    /// reach a projection or pending-request reducer.
    #[must_use]
    pub fn has_consistent_failure_metadata(&self) -> bool {
        let Err(failure) = &self.outcome else {
            return true;
        };
        failure.request_id == self.request_id
            && failure.capability == self.capability
            && failure.provider == self.provider
            && failure.sequence == self.sequence
            && failure.observed_at_ms == self.observed_at_ms
    }
}
