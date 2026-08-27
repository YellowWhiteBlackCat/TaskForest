use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use super::*;

#[test]
fn cpu_is_constructible_and_observable_without_storage_network_or_gpu() {
    let mut cpu = LinuxCpuTelemetryCollector::new();
    let observation = cpu.observe(Instant::now(), 10);

    assert!(
        !observation.sources().is_empty(),
        "the CPU collector must publish its own physical source truth"
    );
}

#[test]
fn slow_domain_fixture_is_not_a_completion_barrier_for_cpu() {
    let started = Arc::new(Barrier::new(2));
    let completed = Arc::new(AtomicUsize::new(0));
    let (release_tx, release_rx) = mpsc::sync_channel(0);
    let fixture_started = started.clone();
    let fixture_completed = completed.clone();
    let slow_domain = thread::spawn(move || {
        fixture_started.wait();
        release_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("fixture should be released before the bounded deadline");
        fixture_completed.fetch_add(1, Ordering::SeqCst);
    });
    started.wait();

    let mut cpu = LinuxCpuTelemetryCollector::new();
    let observation = cpu.observe(Instant::now(), 20);

    assert!(!observation.sources().is_empty());
    assert_eq!(
        completed.load(Ordering::SeqCst),
        0,
        "CPU observation must return without joining a slow sibling domain"
    );
    release_tx.send(()).expect("release slow fixture");
    slow_domain.join().expect("slow fixture should finish");
}

#[test]
fn provider_state_tracker_is_bounded_to_the_current_registry_surface() {
    let mut tracker = ProviderStateTracker::default();
    let available = SourceStatus {
        provider: ProviderId::borrowed("fixture.old"),
        outcome: SourceOutcome::Available,
        item_count: 1,
    };
    tracker.observe(&[available], 10);
    let replacement = SourceStatus {
        provider: ProviderId::borrowed("fixture.new"),
        outcome: SourceOutcome::Empty,
        item_count: 0,
    };
    let states = tracker.observe(&[replacement], 20);

    assert_eq!(states.len(), 1);
    assert_eq!(states[0].provider.as_str(), "fixture.new");
    assert!(
        !tracker
            .last_success
            .keys()
            .any(|provider| provider.as_str() == "fixture.old")
    );
}

#[test]
fn current_device_from_fallback_provider_survives_inventory_outage_as_partial() {
    let sources = [
        SourceStatus {
            provider: ProviderId::borrowed("fixture.inventory"),
            outcome: SourceOutcome::Unavailable(FailureKind::PermissionDenied),
            item_count: 0,
        },
        SourceStatus {
            provider: ProviderId::borrowed("fixture.runtime"),
            outcome: SourceOutcome::Available,
            item_count: 1,
        },
    ];

    assert_eq!(
        device_quality(
            SourceOutcome::Unavailable(FailureKind::PermissionDenied),
            true,
            &sources,
        ),
        SourceQuality::Partial(FailureKind::PermissionDenied)
    );
    assert_eq!(
        device_quality(
            SourceOutcome::Unavailable(FailureKind::PermissionDenied),
            false,
            &sources,
        ),
        SourceQuality::Unavailable(FailureKind::PermissionDenied)
    );
}
