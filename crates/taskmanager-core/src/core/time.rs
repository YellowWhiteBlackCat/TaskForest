//! Pure wall-clock conversion and injected local-time rules.

use std::time::{SystemTime, UNIX_EPOCH};

mod local;

pub use local::{
    LocalDateTime, LocalTimeOffset, LocalTimeRules, LocalTimeRulesCacheKey, LocalTimeRulesChange,
    LocalTimeRulesError, LocalTimeRulesObservation, MAX_LOCAL_TIME_RULE_BYTES,
};

/// Milliseconds since the Unix epoch (0 if the supplied fact is before it).
#[must_use]
pub fn unix_millis(now: SystemTime) -> u64 {
    now.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

/// Microseconds since the Unix epoch (0 if the supplied fact is before it).
#[must_use]
pub fn unix_micros(now: SystemTime) -> u64 {
    now.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "../../tests/headless/core_core_time_tests.rs"]
mod tests;
