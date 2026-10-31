//! Source-level truth for composite domain observations.

use serde::{Deserialize, Serialize};

use super::{FailureKind, ProviderId};

/// Outcome of one independently fallible source in a mixed observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", content = "failure", rename_all = "snake_case")]
pub enum SourceOutcome {
    Available,
    Empty,
    Partial(FailureKind),
    Unavailable(FailureKind),
}

/// Diagnostics for one independently fallible source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceStatus {
    pub provider: ProviderId,
    pub outcome: SourceOutcome,
    pub item_count: usize,
}
