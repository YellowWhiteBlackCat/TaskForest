//! Shared publication-liveness budget for native conformance and root smoke.

use std::time::Duration;

/// This proves eventual publication on a loaded host, rather than measuring
/// collector latency. Parser, scheduler and throughput tests own tighter bounds.
pub const DRAIN_DEADLINE: Duration = Duration::from_secs(30);
pub const DRAIN_POLL: Duration = Duration::from_millis(5);
