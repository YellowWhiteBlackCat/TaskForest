use super::*;

fn sample(completed_at_ms: u64, value: f64) -> HistoricalSample {
    HistoricalSample {
        revision: completed_at_ms,
        completed_at_ms,
        measured_at_ms: Some(completed_at_ms),
        value: Some(value),
    }
}

#[test]
fn ttl_drops_only_past_the_floor_and_keeps_future_dated_samples() {
    let samples = vec![
        sample(1_000, 1.0),
        sample(5_000, 2.0),
        sample(9_000, 3.0),
        // A clock step backwards recorded a "future" completion time.
        sample(20_000, 4.0),
    ];
    let kept = retain_by_ttl(&samples, 10_000, 5_000);
    let kept_times: Vec<u64> = kept.iter().map(|sample| sample.completed_at_ms).collect();
    assert_eq!(kept_times, vec![5_000, 9_000, 20_000]);
}

#[test]
fn ttl_floor_saturates_at_zero_without_panicking() {
    let samples = vec![sample(0, 1.0), sample(1, 2.0)];
    let kept = retain_by_ttl(&samples, 0, u64::MAX);
    assert_eq!(kept.len(), 2);
    let kept = retain_by_ttl(&samples, u64::MAX, u64::MAX);
    // `u64::MAX - ttl` floors at 0 by saturation; everything survives
    // except genuinely pre-floor stamps, of which there are none here.
    assert_eq!(kept.len(), 2);
}

#[test]
fn halving_keeps_the_newest_half_in_order() {
    let samples: Vec<HistoricalSample> = (0..5).map(|index| sample(index * 100, 1.0)).collect();
    let kept = halve_newest(&samples);
    let kept_times: Vec<u64> = kept.iter().map(|sample| sample.completed_at_ms).collect();
    assert_eq!(kept_times, vec![200, 300, 400]);
    assert_eq!(halve_newest(&[]), Vec::<HistoricalSample>::new());
    let single = vec![sample(7, 1.0)];
    assert_eq!(
        halve_newest(&single).len(),
        1,
        "a single-sample file cannot shrink further"
    );
}

#[test]
fn default_policy_matches_the_roadmap_contract() {
    let policy = RetentionPolicy::default();
    assert_eq!(policy.ttl_ms, 7 * 24 * 60 * 60 * 1000);
    assert_eq!(policy.max_bytes, 500 * 1024 * 1024);
}
