//! Bounded service-log worker channel regressions.

use super::*;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use taskmanager_core::ServiceId;

#[test]
fn idle_consumer_keeps_result_queue_bounded_and_newest_snapshots_win() {
    let processed: Arc<Mutex<Vec<ServiceId>>> = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&processed);
    let worker = ServiceLogWorker::with_loader(move |service_id| {
        seen.lock().expect("processed log mutex").push(service_id);
        ServiceLogState::Empty
    });

    // Distinct sortable names so the retained order is observable.
    let names: Vec<ServiceId> = (0..24)
        .map(|index| ServiceId::new(format!("bounded-fixture-{index:02}.service")))
        .collect();
    for service_id in &names {
        worker.request(service_id.clone());
        // Pace each request so the instant loader keeps up through the
        // capacity-1 request queue.
        thread::sleep(Duration::from_millis(2));
    }

    // Wait until the worker has processed well past the result capacity and
    // then gone quiet (the enqueue follows the loader call directly, so a
    // quiet period also proves the last snapshot was queued).
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut last_count = 0;
    let mut last_change = Instant::now();
    loop {
        let count = processed.lock().expect("processed log mutex").len();
        if count != last_count {
            last_count = count;
            last_change = Instant::now();
        }
        if last_count >= 8 && last_change.elapsed() >= Duration::from_millis(200) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "worker processed only {last_count} of {} requests",
            names.len()
        );
        thread::sleep(Duration::from_millis(2));
    }

    // The queue stays at its small capacity even though nothing drained it.
    let queued = worker.result_rx.len();
    assert!(
        queued <= SERVICE_LOG_RESULT_CAPACITY,
        "result queue held {queued} snapshots for an idle consumer"
    );

    // Drop-oldest: the queue retains exactly the newest processed snapshots,
    // in order, so the consumer still sees the freshest state.
    let mut settled = processed.lock().expect("processed log mutex").clone();
    assert!(settled.len() > SERVICE_LOG_RESULT_CAPACITY);
    let keep = SERVICE_LOG_RESULT_CAPACITY;
    let expected: Vec<ServiceId> = settled.split_off(settled.len() - keep);
    let mut drained: Vec<ServiceId> = Vec::new();
    while let Ok(snapshot) = worker.result_rx.try_recv() {
        drained.push(snapshot.service_id);
    }
    assert_eq!(drained, expected);
}
