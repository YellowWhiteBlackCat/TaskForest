//! Shared freshness model for the independently scheduled system telemetry
//! domains: the `SystemObservationState` enum and the internal
//! `SystemDomainValue<T>` carrier holding value, contributing sources, and
//! per-state failure.

use serde::{Deserialize, Serialize};

use crate::core::{FailureKind, SourceStatus};

/// Freshness of one independently scheduled system telemetry domain.
///
/// `Partial` is still a current observation: its value was measured at
/// `observed_at_ms`, while at least one contributing source failed. `Stale`
/// retains a last-known value only for history and diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SystemObservationState {
    #[default]
    Unknown,
    Current {
        observed_at_ms: u64,
    },
    Partial {
        observed_at_ms: u64,
        failure: FailureKind,
    },
    Stale {
        last_success_ms: u64,
        failure: FailureKind,
    },
    Unavailable {
        failure: FailureKind,
    },
}

impl SystemObservationState {
    #[must_use]
    pub const fn is_current(self) -> bool {
        matches!(self, Self::Current { .. } | Self::Partial { .. })
    }

    #[must_use]
    pub const fn observed_at_ms(self) -> Option<u64> {
        match self {
            Self::Current { observed_at_ms } | Self::Partial { observed_at_ms, .. } => {
                Some(observed_at_ms)
            }
            Self::Unknown | Self::Stale { .. } | Self::Unavailable { .. } => None,
        }
    }

    #[must_use]
    pub const fn last_success_ms(self) -> Option<u64> {
        match self {
            Self::Current { observed_at_ms } | Self::Partial { observed_at_ms, .. } => {
                Some(observed_at_ms)
            }
            Self::Stale {
                last_success_ms, ..
            } => Some(last_success_ms),
            Self::Unknown | Self::Unavailable { .. } => None,
        }
    }

    #[must_use]
    pub const fn failure(self) -> Option<FailureKind> {
        match self {
            Self::Partial { failure, .. }
            | Self::Stale { failure, .. }
            | Self::Unavailable { failure } => Some(failure),
            Self::Unknown | Self::Current { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(super) enum SystemDomainValue<T> {
    #[default]
    Unknown,
    Current {
        value: T,
        observed_at_ms: u64,
        sources: Vec<SourceStatus>,
    },
    Partial {
        value: T,
        observed_at_ms: u64,
        failure: FailureKind,
        sources: Vec<SourceStatus>,
    },
    Stale {
        last_value: T,
        last_success_ms: u64,
        failure: FailureKind,
        sources: Vec<SourceStatus>,
    },
    Unavailable {
        failure: FailureKind,
        sources: Vec<SourceStatus>,
    },
}

impl<T> SystemDomainValue<T> {
    pub(super) fn current(value: T, observed_at_ms: u64, sources: Vec<SourceStatus>) -> Self {
        Self::Current {
            value,
            observed_at_ms,
            sources: sorted_sources(sources),
        }
    }

    pub(super) fn partial(
        value: T,
        observed_at_ms: u64,
        failure: FailureKind,
        sources: Vec<SourceStatus>,
    ) -> Self {
        Self::Partial {
            value,
            observed_at_ms,
            failure,
            sources: sorted_sources(sources),
        }
    }

    pub(super) fn stale(
        last_value: T,
        last_success_ms: u64,
        failure: FailureKind,
        sources: Vec<SourceStatus>,
    ) -> Self {
        Self::Stale {
            last_value,
            last_success_ms,
            failure,
            sources: sorted_sources(sources),
        }
    }

    pub(super) fn unavailable(failure: FailureKind, sources: Vec<SourceStatus>) -> Self {
        Self::Unavailable {
            failure,
            sources: sorted_sources(sources),
        }
    }

    pub(super) const fn state(&self) -> SystemObservationState {
        match self {
            Self::Unknown => SystemObservationState::Unknown,
            Self::Current { observed_at_ms, .. } => SystemObservationState::Current {
                observed_at_ms: *observed_at_ms,
            },
            Self::Partial {
                observed_at_ms,
                failure,
                ..
            } => SystemObservationState::Partial {
                observed_at_ms: *observed_at_ms,
                failure: *failure,
            },
            Self::Stale {
                last_success_ms,
                failure,
                ..
            } => SystemObservationState::Stale {
                last_success_ms: *last_success_ms,
                failure: *failure,
            },
            Self::Unavailable { failure, .. } => {
                SystemObservationState::Unavailable { failure: *failure }
            }
        }
    }

    pub(super) const fn current_value(&self) -> Option<&T> {
        match self {
            Self::Current { value, .. } | Self::Partial { value, .. } => Some(value),
            Self::Unknown | Self::Stale { .. } | Self::Unavailable { .. } => None,
        }
    }

    pub(super) const fn last_known_value(&self) -> Option<&T> {
        match self {
            Self::Current { value, .. } | Self::Partial { value, .. } => Some(value),
            Self::Stale { last_value, .. } => Some(last_value),
            Self::Unknown | Self::Unavailable { .. } => None,
        }
    }

    pub(super) fn sources(&self) -> &[SourceStatus] {
        match self {
            Self::Unknown => &[],
            Self::Current { sources, .. }
            | Self::Partial { sources, .. }
            | Self::Stale { sources, .. }
            | Self::Unavailable { sources, .. } => sources,
        }
    }
}

fn sorted_sources(mut sources: Vec<SourceStatus>) -> Vec<SourceStatus> {
    sources.sort_by(|left, right| left.provider.cmp(&right.provider));
    sources
}
