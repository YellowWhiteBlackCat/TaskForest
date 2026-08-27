//! Per-interface network counter baselines and typed rate observations.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use taskmanager_core::{
    DeviceId, FailureKind, ScalarAvailability, ScalarObservation, SourceOutcome,
};

use super::sources::SysfsInterface;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct RawCounterSample {
    pub(super) total_rx_bytes: u64,
    pub(super) total_tx_bytes: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct CounterValues {
    pub(super) total_rx_bytes: ScalarObservation<u64>,
    pub(super) total_tx_bytes: ScalarObservation<u64>,
    pub(super) rx_rate: ScalarObservation<u64>,
    pub(super) tx_rate: ScalarObservation<u64>,
    pub(super) utilization: ScalarObservation<f32>,
}

impl CounterValues {
    pub(super) fn unavailable(failure: FailureKind) -> Self {
        Self {
            total_rx_bytes: ScalarObservation::unavailable(failure),
            total_tx_bytes: ScalarObservation::unavailable(failure),
            rx_rate: ScalarObservation::unavailable(failure),
            tx_rate: ScalarObservation::unavailable(failure),
            utilization: ScalarObservation::unavailable(failure),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct NetworkCounterState {
    trackers: HashMap<Arc<str>, InterfaceCounterTracker>,
    awaiting_reappearance: HashSet<Arc<str>>,
}

impl NetworkCounterState {
    pub(super) fn reset_absent(&mut self, device_ids: &[DeviceId]) {
        for device_id in device_ids {
            self.trackers.remove(device_id.as_str());
            self.awaiting_reappearance
                .insert(Arc::from(device_id.as_str()));
        }
    }

    pub(super) fn confirm_reappeared(&mut self, device_ids: &[DeviceId]) {
        for device_id in device_ids {
            if !self.awaiting_reappearance.remove(device_id.as_str()) {
                // Defensive fallback: a lifecycle generation advanced without
                // a matching absence receipt in this provider.
                self.trackers.remove(device_id.as_str());
            }
        }
    }

    pub(super) fn expire(&mut self, device_ids: &[DeviceId]) {
        for device_id in device_ids {
            self.trackers.remove(device_id.as_str());
            self.awaiting_reappearance.remove(device_id.as_str());
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct CounterObservation {
    pub(super) value: HashMap<Arc<str>, CounterValues>,
    pub(super) outcome: SourceOutcome,
    pub(super) current_count: usize,
}

#[derive(Clone, Debug)]
struct InterfaceCounterTracker {
    native_name: String,
    baseline: Option<CounterBaseline>,
    total_rx_bytes: ScalarObservation<u64>,
    total_tx_bytes: ScalarObservation<u64>,
    rx_rate: ScalarObservation<u64>,
    tx_rate: ScalarObservation<u64>,
    utilization: ScalarObservation<f32>,
}

#[derive(Clone, Copy, Debug)]
struct CounterBaseline {
    observed_at: Instant,
    total_rx_bytes: u64,
    total_tx_bytes: u64,
}

impl InterfaceCounterTracker {
    fn new(native_name: &str) -> Self {
        Self {
            native_name: native_name.to_owned(),
            baseline: None,
            total_rx_bytes: ScalarObservation::default(),
            total_tx_bytes: ScalarObservation::default(),
            rx_rate: ScalarObservation::default(),
            tx_rate: ScalarObservation::default(),
            utilization: ScalarObservation::default(),
        }
    }

    fn observe(
        &mut self,
        sample: RawCounterSample,
        link_speed: ScalarObservation<u64>,
        link_up: ScalarObservation<bool>,
        now: Instant,
        now_ms: u64,
    ) -> CounterValues {
        let baseline = self.baseline;
        let rate_window = baseline.map_or(RateWindow::FirstSample, |baseline| {
            now.checked_duration_since(baseline.observed_at).map_or(
                RateWindow::Invalid(FailureKind::IdentityChanged),
                |elapsed| {
                    let seconds = elapsed.as_secs_f64();
                    if seconds > 0.0 {
                        RateWindow::Valid(seconds)
                    } else {
                        RateWindow::Invalid(FailureKind::TemporarilyUnavailable)
                    }
                },
            )
        });
        let (total_rx_bytes, mut rx_rate) = observe_direction(
            self.total_rx_bytes,
            self.rx_rate,
            sample.total_rx_bytes,
            baseline.map(|baseline| baseline.total_rx_bytes),
            rate_window,
            now_ms,
        );
        let (total_tx_bytes, mut tx_rate) = observe_direction(
            self.total_tx_bytes,
            self.tx_rate,
            sample.total_tx_bytes,
            baseline.map(|baseline| baseline.total_tx_bytes),
            rate_window,
            now_ms,
        );
        self.baseline = Some(CounterBaseline {
            observed_at: now,
            total_rx_bytes: sample.total_rx_bytes,
            total_tx_bytes: sample.total_tx_bytes,
        });
        self.total_rx_bytes = total_rx_bytes;
        self.total_tx_bytes = total_tx_bytes;

        if matches!(link_up.current_value(), Some(false)) {
            // A down link closes the sampling window. Recovery must establish
            // a fresh baseline instead of averaging across downtime.
            self.baseline = None;
            rx_rate = link_down_rate(rx_rate, total_rx_bytes);
            tx_rate = link_down_rate(tx_rate, total_tx_bytes);
        }
        self.rx_rate = rx_rate;
        self.tx_rate = tx_rate;
        self.utilization = observe_utilization(
            self.utilization,
            self.rx_rate,
            self.tx_rate,
            link_speed,
            now_ms,
        );
        self.values()
    }

    fn fail(&mut self, failure: FailureKind) -> CounterValues {
        // The next successful provider sample establishes a new baseline. Its
        // delta must never be divided across this failed interval.
        self.baseline = None;
        self.total_rx_bytes = self.total_rx_bytes.transition_failure(failure);
        self.total_tx_bytes = self.total_tx_bytes.transition_failure(failure);
        self.rx_rate = self.rx_rate.transition_failure(failure);
        self.tx_rate = self.tx_rate.transition_failure(failure);
        self.utilization = self.utilization.transition_failure(failure);
        self.values()
    }

    fn values(&self) -> CounterValues {
        CounterValues {
            total_rx_bytes: self.total_rx_bytes,
            total_tx_bytes: self.total_tx_bytes,
            rx_rate: self.rx_rate,
            tx_rate: self.tx_rate,
            utilization: self.utilization,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum RateWindow {
    FirstSample,
    Valid(f64),
    Invalid(FailureKind),
}

fn observe_direction(
    previous_total: ScalarObservation<u64>,
    previous_rate: ScalarObservation<u64>,
    current_total: u64,
    baseline_total: Option<u64>,
    rate_window: RateWindow,
    now_ms: u64,
) -> (ScalarObservation<u64>, ScalarObservation<u64>) {
    let Some(baseline_total) = baseline_total else {
        return (
            ScalarObservation::available(current_total, now_ms),
            ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable)
                .retain_previous(previous_rate),
        );
    };
    let Some(delta) = current_total.checked_sub(baseline_total) else {
        return (
            ScalarObservation::unavailable(FailureKind::IdentityChanged)
                .retain_previous(previous_total),
            ScalarObservation::unavailable(FailureKind::IdentityChanged)
                .retain_previous(previous_rate),
        );
    };
    let total = ScalarObservation::available(current_total, now_ms);
    let observed_rate = match rate_window {
        RateWindow::Valid(seconds) => ScalarObservation::available(rate(delta, seconds), now_ms),
        RateWindow::FirstSample => {
            ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable)
                .retain_previous(previous_rate)
        }
        RateWindow::Invalid(failure) => {
            ScalarObservation::unavailable(failure).retain_previous(previous_rate)
        }
    };
    (total, observed_rate)
}

fn link_down_rate(
    observed_rate: ScalarObservation<u64>,
    observed_total: ScalarObservation<u64>,
) -> ScalarObservation<u64> {
    let failure = if observed_total.availability().failure() == Some(FailureKind::IdentityChanged) {
        FailureKind::IdentityChanged
    } else {
        FailureKind::TemporarilyUnavailable
    };
    observed_rate.transition_failure(failure)
}

pub(super) fn observe_counter_samples(
    state: &mut NetworkCounterState,
    inventory: &[SysfsInterface],
    fresh_interfaces: &HashSet<Arc<str>>,
    discovery_outcome: SourceOutcome,
    samples: &HashMap<Arc<str>, RawCounterSample>,
    now: Instant,
    now_ms: u64,
) -> CounterObservation {
    let mut values = HashMap::new();
    let mut current_ids = HashSet::new();
    for interface in inventory {
        let stable_id = interface.stable_id.clone();
        current_ids.insert(stable_id.clone());
        let tracker = state
            .trackers
            .entry(stable_id)
            .or_insert_with(|| InterfaceCounterTracker::new(interface.name.as_ref()));
        if tracker.native_name.as_str() != interface.name.as_ref() {
            *tracker = InterfaceCounterTracker::new(interface.name.as_ref());
        }
        let observed = if !fresh_interfaces.contains(&interface.name) {
            tracker.fail(
                discovery_outcome
                    .failure()
                    .unwrap_or(FailureKind::TemporarilyUnavailable),
            )
        } else if let Some(sample) = samples.get(&interface.name) {
            tracker.observe(
                *sample,
                interface.link_speed,
                interface.link_up,
                now,
                now_ms,
            )
        } else {
            tracker.fail(FailureKind::TemporarilyUnavailable)
        };
        values.insert(interface.name.clone(), observed);
    }

    if matches!(
        discovery_outcome,
        SourceOutcome::Available | SourceOutcome::Empty
    ) {
        // An authoritative disappearance closes the attachment generation.
        // Reappearance of the same stable ID must start with a fresh baseline.
        state
            .trackers
            .retain(|stable_id, _| current_ids.contains(stable_id));
    }

    summarize(values, inventory.len())
}

fn summarize(
    value: HashMap<Arc<str>, CounterValues>,
    inventory_count: usize,
) -> CounterObservation {
    if inventory_count == 0 {
        return CounterObservation {
            value,
            outcome: SourceOutcome::Empty,
            current_count: 0,
        };
    }
    let current_count = value
        .values()
        .filter(|counter| {
            counter.rx_rate.availability().is_current()
                && counter.tx_rate.availability().is_current()
        })
        .count();
    let failure = value
        .values()
        .flat_map(|counter| {
            [
                counter.rx_rate.availability().failure(),
                counter.tx_rate.availability().failure(),
            ]
        })
        .flatten()
        .reduce(select_failure)
        .unwrap_or(FailureKind::TemporarilyUnavailable);
    let any_partial = value.values().any(|counter| {
        matches!(
            counter.rx_rate.availability(),
            ScalarAvailability::Partial(_)
        ) || matches!(
            counter.tx_rate.availability(),
            ScalarAvailability::Partial(_)
        )
    });
    let outcome = if current_count == inventory_count && !any_partial {
        SourceOutcome::Available
    } else if current_count == 0 {
        SourceOutcome::Unavailable(failure)
    } else {
        SourceOutcome::Partial(failure)
    };
    CounterObservation {
        value,
        outcome,
        current_count,
    }
}

fn observe_utilization(
    previous: ScalarObservation<f32>,
    rx_rate: ScalarObservation<u64>,
    tx_rate: ScalarObservation<u64>,
    link_speed: ScalarObservation<u64>,
    now_ms: u64,
) -> ScalarObservation<f32> {
    let Some(link_speed_mbps) = link_speed.current_value().copied() else {
        let failure = link_speed
            .availability()
            .failure()
            .unwrap_or(FailureKind::Unsupported);
        return ScalarObservation::unavailable(failure).retain_previous(previous);
    };
    let (Some(rx_bytes_per_sec), Some(tx_bytes_per_sec)) = (
        rx_rate.current_value().copied(),
        tx_rate.current_value().copied(),
    ) else {
        let failure = rx_rate
            .availability()
            .failure()
            .or(tx_rate.availability().failure())
            .unwrap_or(FailureKind::TemporarilyUnavailable);
        return ScalarObservation::unavailable(failure).retain_previous(previous);
    };
    let Some(value) = link_utilization_pct(rx_bytes_per_sec, tx_bytes_per_sec, link_speed_mbps)
    else {
        // A present-but-zero link speed (sysfs reads 0 while the interface is
        // down) leaves no measurable capacity: 0 bytes/s would divide into a
        // NaN and any real rate into an `inf` clamped to a fake 100%. The
        // ratio is a typed gap instead; the rate observations stand as read.
        return ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable)
            .retain_previous(previous);
    };
    let partial_failure = [
        link_speed.availability(),
        rx_rate.availability(),
        tx_rate.availability(),
    ]
    .into_iter()
    .find_map(|availability| match availability {
        ScalarAvailability::Partial(failure) => Some(failure),
        _ => None,
    });
    match partial_failure {
        Some(failure) => ScalarObservation::partial(value, now_ms, failure),
        None => ScalarObservation::available(value, now_ms),
    }
}

fn rate(delta_bytes: u64, elapsed_seconds: f64) -> u64 {
    (delta_bytes as f64 / elapsed_seconds) as u64
}

/// Percent of link capacity used by rx+tx. This is the single source of truth
/// for the utilization ratio: the counter path (`observe_utilization`) and the
/// post-counter WiFi tx-bitrate backfill (`super::recompute_utilization`) both
/// route through here so the formula cannot drift between the two.
///
/// Returns `None` when the capacity is not positive (sysfs reports speed 0
/// for a down interface): with no denominator the ratio is undefined, not
/// zero and not 100%.
pub(super) fn link_utilization_pct(
    rx_bytes_per_sec: u64,
    tx_bytes_per_sec: u64,
    link_speed_mbps: u64,
) -> Option<f32> {
    if link_speed_mbps == 0 {
        return None;
    }
    let bytes_per_second = rx_bytes_per_sec.saturating_add(tx_bytes_per_sec);
    let capacity_bits_per_second = link_speed_mbps as f64 * 1_000_000.0;
    Some(
        (bytes_per_second as f64 * 8.0 / capacity_bits_per_second * 100.0).clamp(0.0, 100.0) as f32,
    )
}

const fn select_failure(left: FailureKind, right: FailureKind) -> FailureKind {
    if failure_priority(right) > failure_priority(left) {
        right
    } else {
        left
    }
}

const fn failure_priority(failure: FailureKind) -> u8 {
    match failure {
        FailureKind::RequiresEscalation => 9,
        FailureKind::PermissionDenied => 8,
        FailureKind::ProviderFault => 7,
        FailureKind::TimedOut => 6,
        FailureKind::TemporarilyUnavailable => 5,
        FailureKind::MissingDependency => 4,
        FailureKind::IdentityChanged => 3,
        FailureKind::Rejected => 2,
        FailureKind::Unsupported => 1,
    }
}

trait SourceOutcomeFailure {
    fn failure(self) -> Option<FailureKind>;
}

impl SourceOutcomeFailure for SourceOutcome {
    fn failure(self) -> Option<FailureKind> {
        match self {
            SourceOutcome::Partial(failure) | SourceOutcome::Unavailable(failure) => Some(failure),
            SourceOutcome::Available | SourceOutcome::Empty => None,
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/headless/engine/collector/network/counters.rs"]
mod tests;
