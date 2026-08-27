//! Retention policy: TTL and quota trimming (roadmap #4 defaults: 7 days /
//! 500 MB, both tested as pure functions over parsed samples).

use taskmanager_core::HistoricalSample;

/// Bounding policy for the on-disk history. The defaults are the roadmap #4
/// contract; a stricter policy is always allowed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetentionPolicy {
    /// Samples older than this (by completion wall-clock) are dropped at trim
    /// time. Future-dated samples (clock stepped back) are never expired.
    pub ttl_ms: u64,
    /// Upper bound on the total size of all series files. When exceeded, the
    /// oldest series files are halved (oldest samples dropped) until the
    /// directory fits. If minimum one-sample files still exceed the quota,
    /// whole oldest series are retired so this remains a strict bound.
    pub max_bytes: u64,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            ttl_ms: 7 * 24 * 60 * 60 * 1000,
            max_bytes: 500 * 1024 * 1024,
        }
    }
}

impl RetentionPolicy {
    /// A policy suitable for tests: bounded in both axes with explicit
    /// numbers, so trim behavior is observable with a handful of samples.
    #[must_use]
    pub const fn for_tests(ttl_ms: u64, max_bytes: u64) -> Self {
        Self { ttl_ms, max_bytes }
    }
}

/// Keep only samples whose completion time is within `ttl_ms` of `now_ms`.
/// Future-dated samples (a clock that stepped backwards) are always kept:
/// their wall clock is *ahead*, so expiry by age cannot apply, and rewriting
/// them would hide the jump the query layer is supposed to surface.
#[must_use]
pub fn retain_by_ttl(
    samples: &[HistoricalSample],
    now_ms: u64,
    ttl_ms: u64,
) -> Vec<HistoricalSample> {
    let floor = now_ms.saturating_sub(ttl_ms);
    samples
        .iter()
        .copied()
        .filter(|sample| sample.completed_at_ms >= floor)
        .collect()
}

/// The half of `samples` with the newest completion times, for quota relief.
/// Order is preserved. A single-sample file cannot shrink — quota relief then
/// moves on to the next file instead of looping on it.
#[must_use]
pub fn halve_newest(samples: &[HistoricalSample]) -> Vec<HistoricalSample> {
    let keep_from = samples.len() / 2;
    samples[keep_from..].to_vec()
}

#[cfg(test)]
#[path = "../tests/headless/retention.rs"]
mod tests;
