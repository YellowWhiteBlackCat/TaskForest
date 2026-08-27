//! Linux-only verification of the live `accesskit_unix` bridge.
//!
//! These tests confirm the adapter constructs and degrades gracefully when no
//! assistive technology is active — the documented safe-by-design behavior of
//! `accesskit_unix::Adapter`. They require no D-Bus, no Orca, and no special
//! skip logic: when `org.a11y.Bus IsEnabled` is false (the host default),
//! `update_if_active` is literally a no-op that does not invoke its factory.
//!
//! Proving the tree is *observable on the AT-SPI bus* needs a live at-spi2
//! session with accessibility enabled plus a bus walker (Orca or a gdbus/atspi
//! probe acting as the AT client); that tier is intentionally external to the
//! unit suite.

#![cfg(target_os = "linux")]

use taskmanager_accessibility_linux::LinuxAccessKitBridge;
use taskmanager_ui_contract::{
    AccessibilityBridge, AccessibilityBridgeStatus, GraphSummary, ProcessRowInput,
    SemanticSnapshotBuilder,
};

fn sample_snapshot() -> taskmanager_ui_contract::SemanticSnapshot {
    SemanticSnapshotBuilder::new(1)
        .application_name("TaskForest")
        .process_row(ProcessRowInput {
            id: String::from("1024"),
            name: String::from("firefox"),
            cpu_percent: Some(12.0),
            memory_percent: Some(4.0),
            selected: true,
        })
        .cpu_graph(GraphSummary {
            current: 18.0,
            peak: 72.0,
            maximum: 100.0,
        })
        .status_announcement("TaskForest ready")
        .build()
        .expect("sample snapshot well-formed")
}

#[test]
fn bridge_constructs_without_panic_on_a_headless_host() {
    // Constructing the adapter spawns accesskit_unix's process-global worker
    // thread. With no session bus (or with accessibility disabled), that thread
    // simply never connects; construction must not panic or block.
    let bridge = LinuxAccessKitBridge::new();

    // The bridge is initialized and able to publish. It does not claim support
    // it cannot deliver: it reports a real adapter, not a bus marker.
    let capability = bridge.capability();
    assert_eq!(capability.status(), AccessibilityBridgeStatus::Ready);
    assert!(capability.features().tables);
    assert!(capability.features().live_regions);
    assert!(capability.features().actions);
}

#[test]
fn publish_is_a_safe_no_op_until_an_assistive_technology_subscribes() {
    let bridge = LinuxAccessKitBridge::new();
    let snapshot = sample_snapshot();
    let revision = snapshot.revision();

    // On a host without an active AT, this drives the inactive adapter path:
    // the factory closure is never invoked and no bus traffic occurs. It must
    // still report the publication honestly.
    let publication = bridge
        .try_publish(snapshot)
        .expect("publish must succeed even with no AT listening");
    assert_eq!(publication.snapshot_revision, revision);
}

#[test]
fn action_queue_starts_empty_and_drains_to_none() {
    let bridge = LinuxAccessKitBridge::new();
    // No AT is connected, so no actions can ever have been enqueued.
    let drained = bridge
        .try_recv_action()
        .expect("drain must not error with no AT listening");
    assert!(drained.is_none());
}

#[test]
fn repeated_publish_and_drain_cycles_do_not_panic() {
    // The process-global worker thread is shared across bridge instances
    // (OnceLock). Exercise several construct/publish/drain cycles to confirm
    // the shared state stays consistent.
    for revision in 1..=5 {
        let bridge = LinuxAccessKitBridge::new();
        let snapshot = SemanticSnapshotBuilder::new(revision)
            .application_name("TaskForest")
            .cpu_graph(GraphSummary {
                current: 10.0,
                peak: 20.0,
                maximum: 100.0,
            })
            .build()
            .expect("snapshot well-formed");
        let _ = bridge.try_publish(snapshot);
        let _ = bridge.try_recv_action();
    }
}
