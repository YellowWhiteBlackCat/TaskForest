use std::time::Duration;

use super::*;

fn interface(name: &str, mac: &str, link_speed: ScalarObservation<u64>) -> SysfsInterface {
    SysfsInterface {
        stable_id: Arc::from(format!("net:mac:{}", mac.to_lowercase())),
        name: Arc::from(name),
        arp_type: Some(1),
        mac_addr: Some(Arc::from(mac)),
        link_speed,
        link_up: ScalarObservation::available(true, 10),
        driver: None,
        adapter: None,
    }
}

fn samples(entries: &[(&str, u64, u64)]) -> HashMap<Arc<str>, RawCounterSample> {
    entries
        .iter()
        .map(|(name, rx, tx)| {
            (
                Arc::from(*name),
                RawCounterSample {
                    total_rx_bytes: *rx,
                    total_tx_bytes: *tx,
                },
            )
        })
        .collect()
}

fn observe(
    state: &mut NetworkCounterState,
    inventory: &[SysfsInterface],
    samples: HashMap<Arc<str>, RawCounterSample>,
    now: Instant,
    now_ms: u64,
) -> CounterObservation {
    observe_counter_samples(
        state,
        inventory,
        &inventory
            .iter()
            .map(|interface| interface.name.clone())
            .collect(),
        SourceOutcome::Available,
        &samples,
        now,
        now_ms,
    )
}

#[test]
fn first_sample_is_unavailable_and_second_idle_sample_is_real_zero() {
    let mut state = NetworkCounterState::default();
    let start = Instant::now();
    let inventory = [interface(
        "enp1s0",
        "aa:bb:cc:dd:ee:01",
        ScalarObservation::available(1_000, 10),
    )];
    let first = observe(
        &mut state,
        &inventory,
        samples(&[("enp1s0", 100, 50)]),
        start,
        10,
    );
    let second = observe(
        &mut state,
        &inventory,
        samples(&[("enp1s0", 100, 50)]),
        start + Duration::from_secs(1),
        20,
    );

    assert_eq!(
        first.value["enp1s0"].rx_rate.availability(),
        ScalarAvailability::Unavailable(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(first.value["enp1s0"].rx_rate.current_value(), None);
    assert_eq!(
        first.value["enp1s0"].total_rx_bytes,
        ScalarObservation::available(100, 10)
    );
    assert_eq!(
        second.value["enp1s0"].rx_rate,
        ScalarObservation::available(0, 20)
    );
    assert_eq!(
        second.value["enp1s0"].utilization,
        ScalarObservation::available(0.0, 20)
    );
}

#[test]
fn unavailable_link_capacity_is_not_zero_utilization() {
    let mut state = NetworkCounterState::default();
    let start = Instant::now();
    let inventory = [interface(
        "enp1s0",
        "aa:bb:cc:dd:ee:01",
        ScalarObservation::unavailable(FailureKind::Unsupported),
    )];
    observe(
        &mut state,
        &inventory,
        samples(&[("enp1s0", 1, 1)]),
        start,
        10,
    );
    let second = observe(
        &mut state,
        &inventory,
        samples(&[("enp1s0", 1, 1)]),
        start + Duration::from_secs(1),
        20,
    );

    assert_eq!(
        second.value["enp1s0"].utilization.availability(),
        ScalarAvailability::Unavailable(FailureKind::Unsupported)
    );
    assert_eq!(second.value["enp1s0"].utilization.current_value(), None);
}

/// sysfs reports speed 0 for a down interface: a present-but-zero capacity
/// must yield a typed unavailable ratio — never a NaN (0/0 wrapped as
/// available) nor a saturated fake 100% — while the rate side stands as read.
#[test]
fn zero_link_capacity_is_unavailable_utilization_not_nan_or_saturated() {
    let mut state = NetworkCounterState::default();
    let start = Instant::now();
    let inventory = [interface(
        "enp1s0",
        "aa:bb:cc:dd:ee:01",
        ScalarObservation::available(0, 10),
    )];
    observe(
        &mut state,
        &inventory,
        samples(&[("enp1s0", 100, 50)]),
        start,
        10,
    );
    let second = observe(
        &mut state,
        &inventory,
        samples(&[("enp1s0", 1_000_100, 500_050)]),
        start + Duration::from_secs(1),
        20,
    );

    // The rate side is unaffected by the degenerate link speed.
    assert_eq!(
        second.value["enp1s0"].rx_rate,
        ScalarObservation::available(1_000_000, 20)
    );
    assert_eq!(
        second.value["enp1s0"].tx_rate,
        ScalarObservation::available(500_000, 20)
    );
    assert_eq!(
        second.value["enp1s0"].utilization.availability(),
        ScalarAvailability::Unavailable(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(second.value["enp1s0"].utilization.current_value(), None);
}

#[test]
fn provider_failure_is_stale_and_recovery_does_not_cross_failure_window() {
    let mut state = NetworkCounterState::default();
    let start = Instant::now();
    let inventory = [interface(
        "enp1s0",
        "aa:bb:cc:dd:ee:01",
        ScalarObservation::available(1_000, 10),
    )];
    observe(
        &mut state,
        &inventory,
        samples(&[("enp1s0", 100, 50)]),
        start,
        10,
    );
    let success = observe(
        &mut state,
        &inventory,
        samples(&[("enp1s0", 200, 100)]),
        start + Duration::from_secs(1),
        20,
    );
    let failed = observe(
        &mut state,
        &inventory,
        HashMap::new(),
        start + Duration::from_secs(2),
        30,
    );
    let first_recovery = observe(
        &mut state,
        &inventory,
        samples(&[("enp1s0", 1_100, 550)]),
        start + Duration::from_secs(10),
        100,
    );
    let second_recovery = observe(
        &mut state,
        &inventory,
        samples(&[("enp1s0", 1_200, 600)]),
        start + Duration::from_secs(11),
        110,
    );

    assert_eq!(success.value["enp1s0"].rx_rate.current_value(), Some(&100));
    assert_eq!(
        failed.value["enp1s0"].rx_rate.availability(),
        ScalarAvailability::Stale(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(first_recovery.value["enp1s0"].rx_rate.current_value(), None);
    assert_eq!(
        second_recovery.value["enp1s0"].rx_rate.current_value(),
        Some(&100)
    );
}

#[test]
fn reorder_and_confirmed_reconnect_never_share_a_baseline() {
    let mut state = NetworkCounterState::default();
    let start = Instant::now();
    let first = interface(
        "enp1s0",
        "aa:bb:cc:dd:ee:01",
        ScalarObservation::available(1_000, 10),
    );
    let second = interface(
        "enp2s0",
        "aa:bb:cc:dd:ee:02",
        ScalarObservation::available(1_000, 10),
    );
    observe(
        &mut state,
        &[first.clone(), second.clone()],
        samples(&[("enp1s0", 1, 0), ("enp2s0", 2, 0)]),
        start,
        10,
    );
    let reordered = observe(
        &mut state,
        &[second.clone(), first.clone()],
        samples(&[("enp1s0", 11, 0), ("enp2s0", 22, 0)]),
        start + Duration::from_secs(1),
        20,
    );
    observe(
        &mut state,
        &[],
        HashMap::new(),
        start + Duration::from_secs(2),
        30,
    );
    let reconnected = observe(
        &mut state,
        &[first],
        samples(&[("enp1s0", 999, 0)]),
        start + Duration::from_secs(3),
        40,
    );

    assert_eq!(reordered.value["enp1s0"].rx_rate.current_value(), Some(&10));
    assert_eq!(reordered.value["enp2s0"].rx_rate.current_value(), Some(&20));
    assert_eq!(
        reconnected.value["enp1s0"].rx_rate.current_value(),
        None,
        "new lifecycle generation must establish a fresh baseline"
    );
}

#[test]
fn lifecycle_hooks_keep_reappearance_baseline_but_expiry_removes_it() {
    let mut state = NetworkCounterState::default();
    let start = Instant::now();
    let interface = interface(
        "enp1s0",
        "aa:bb:cc:dd:ee:01",
        ScalarObservation::available(1_000, 10),
    );
    let device_id = DeviceId::new("net:mac:aa:bb:cc:dd:ee:01");
    observe(
        &mut state,
        std::slice::from_ref(&interface),
        samples(&[("enp1s0", 100, 50)]),
        start,
        10,
    );

    state.reset_absent(std::slice::from_ref(&device_id));
    let first_reappearance = observe(
        &mut state,
        std::slice::from_ref(&interface),
        samples(&[("enp1s0", 500, 250)]),
        start + Duration::from_secs(1),
        20,
    );
    state.confirm_reappeared(std::slice::from_ref(&device_id));
    let second_reappearance = observe(
        &mut state,
        std::slice::from_ref(&interface),
        samples(&[("enp1s0", 600, 300)]),
        start + Duration::from_secs(2),
        30,
    );
    state.expire(std::slice::from_ref(&device_id));
    let after_expiry = observe(
        &mut state,
        &[interface],
        samples(&[("enp1s0", 700, 350)]),
        start + Duration::from_secs(3),
        40,
    );

    assert_eq!(
        first_reappearance.value["enp1s0"].rx_rate.current_value(),
        None
    );
    assert_eq!(
        second_reappearance.value["enp1s0"].rx_rate.current_value(),
        Some(&100)
    );
    assert_eq!(after_expiry.value["enp1s0"].rx_rate.current_value(), None);
}

#[test]
fn discovery_provider_failure_is_per_interface_and_typed() {
    let mut state = NetworkCounterState::default();
    let start = Instant::now();
    let inventory = [interface(
        "enp1s0",
        "aa:bb:cc:dd:ee:01",
        ScalarObservation::available(1_000, 10),
    )];
    observe(
        &mut state,
        &inventory,
        samples(&[("enp1s0", 100, 50)]),
        start,
        10,
    );
    observe(
        &mut state,
        &inventory,
        samples(&[("enp1s0", 200, 100)]),
        start + Duration::from_secs(1),
        20,
    );

    let denied = observe_counter_samples(
        &mut state,
        &inventory,
        &HashSet::new(),
        SourceOutcome::Unavailable(FailureKind::PermissionDenied),
        &samples(&[("enp1s0", 300, 150)]),
        start + Duration::from_secs(2),
        30,
    );

    assert_eq!(
        denied.value["enp1s0"].rx_rate.availability(),
        ScalarAvailability::Stale(FailureKind::PermissionDenied)
    );
    assert_eq!(denied.value["enp1s0"].rx_rate.current_value(), None);
    assert_eq!(
        denied.value["enp1s0"].rx_rate.last_known_value(),
        Some(&100)
    );
}

#[test]
fn one_direction_counter_rollback_does_not_invalidate_the_other_direction() {
    let mut state = NetworkCounterState::default();
    let start = Instant::now();
    let inventory = [interface(
        "enp1s0",
        "aa:bb:cc:dd:ee:01",
        ScalarObservation::available(1_000, 10),
    )];
    observe(
        &mut state,
        &inventory,
        samples(&[("enp1s0", 100, 50)]),
        start,
        10,
    );
    observe(
        &mut state,
        &inventory,
        samples(&[("enp1s0", 200, 100)]),
        start + Duration::from_secs(1),
        20,
    );
    let rollback = observe(
        &mut state,
        &inventory,
        samples(&[("enp1s0", 10, 150)]),
        start + Duration::from_secs(2),
        30,
    );
    let values = rollback.value["enp1s0"];

    assert_eq!(
        values.total_rx_bytes.availability(),
        ScalarAvailability::Stale(FailureKind::IdentityChanged)
    );
    assert_eq!(values.total_rx_bytes.last_known_value(), Some(&200));
    assert_eq!(
        values.rx_rate.availability(),
        ScalarAvailability::Stale(FailureKind::IdentityChanged)
    );
    assert_eq!(values.total_tx_bytes, ScalarObservation::available(150, 30));
    assert_eq!(values.tx_rate.current_value(), Some(&50));
}

#[test]
fn link_down_closes_rate_window_and_recovery_requires_a_fresh_baseline() {
    let mut state = NetworkCounterState::default();
    let start = Instant::now();
    let mut interface = interface(
        "enp1s0",
        "aa:bb:cc:dd:ee:01",
        ScalarObservation::available(1_000, 10),
    );
    observe(
        &mut state,
        std::slice::from_ref(&interface),
        samples(&[("enp1s0", 100, 50)]),
        start,
        10,
    );
    observe(
        &mut state,
        std::slice::from_ref(&interface),
        samples(&[("enp1s0", 200, 100)]),
        start + Duration::from_secs(1),
        20,
    );
    interface.link_up = ScalarObservation::available(false, 30);
    let down = observe(
        &mut state,
        std::slice::from_ref(&interface),
        samples(&[("enp1s0", 220, 110)]),
        start + Duration::from_secs(2),
        30,
    );
    interface.link_up = ScalarObservation::available(true, 40);
    let first_up = observe(
        &mut state,
        std::slice::from_ref(&interface),
        samples(&[("enp1s0", 500, 250)]),
        start + Duration::from_secs(10),
        40,
    );
    let second_up = observe(
        &mut state,
        std::slice::from_ref(&interface),
        samples(&[("enp1s0", 600, 300)]),
        start + Duration::from_secs(11),
        50,
    );

    assert_eq!(
        down.value["enp1s0"].rx_rate.availability(),
        ScalarAvailability::Stale(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(
        down.value["enp1s0"].total_rx_bytes,
        ScalarObservation::available(220, 30)
    );
    assert_eq!(first_up.value["enp1s0"].rx_rate.current_value(), None);
    assert_eq!(
        second_up.value["enp1s0"].rx_rate.current_value(),
        Some(&100)
    );
}
