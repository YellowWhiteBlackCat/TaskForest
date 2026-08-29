//! test-intent: behavior
//! The inventory cache releases its mutable borrow before invoking a builder,
//! so renderer composition may safely re-enter the same cache. The
//! dual-series cache below additionally pins its invalidation boundary: one
//! pair entry per device identity + family, reused within a history epoch,
//! reloaded when the epoch (accepted writes / visible capacity) moves.

use super::*;

#[test]
fn inventory_builder_can_reenter_without_a_live_refcell_borrow() {
    let cache = RefCell::new(InventoryProjection::<u8>::default());
    let outer = InventoryDataFingerprint {
        watermark: 1,
        source_len: 1,
        sort: None,
    };
    let inner = InventoryDataFingerprint {
        watermark: 2,
        source_len: 1,
        sort: None,
    };

    let (rows, _, _) = project_inventory(
        &cache,
        outer,
        "",
        || {
            let _ = project_inventory(&cache, inner, "", || vec![2], |_, _| true);
            vec![1]
        },
        |_, _| true,
    );

    assert_eq!(rows.as_ref(), &[1]);
    assert!(cache.borrow().matches_data(outer));
}

/// The dual-series pair cache reuses one entry per device identity + family
/// within a history epoch, reloads when the epoch moves (a real accepted
/// write, or a visible-capacity change), and never serves one family or
/// device's pair to another.
#[test]
fn dual_device_series_is_reused_per_epoch_and_never_crosses_keys() {
    let mut app = crate::IcedApp::default();
    let demo = taskmanager_shell::demo_app();
    let snapshot = demo
        .projection()
        .snapshot
        .clone()
        .expect("demo snapshot fixture");
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(snapshot.clone()))),
    );
    // One accepted history frame fixes the starting epoch.
    taskmanager_shell::fixture::record_demo_history_frame(&mut app.shell, &snapshot, None, None);
    let disk = &snapshot.disks[0];
    let disk_id = disk.device_id.clone();
    let generation = disk.device_generation.get();

    let loads = std::cell::Cell::new(0_u32);
    let load = |_: &taskmanager_shell::ShellApp| {
        loads.set(loads.get() + 1);
        (vec![1.0, 2.0], vec![3.0, 4.0])
    };

    let first = app.projection_caches.dual_device_series(
        &app.shell,
        DualDeviceSeriesFamily::DiskReadWrite,
        &disk_id,
        generation,
        load,
    );
    assert_eq!(loads.get(), 1);
    let second = app.projection_caches.dual_device_series(
        &app.shell,
        DualDeviceSeriesFamily::DiskReadWrite,
        &disk_id,
        generation,
        load,
    );
    assert_eq!(
        loads.get(),
        1,
        "the same epoch must reuse the cached pair without reloading"
    );
    assert!(Rc::ptr_eq(&first.0, &second.0));
    assert!(Rc::ptr_eq(&first.1, &second.1));

    // A different family for the same device is a different key: it reloads
    // (the NIC pair never borrows the disk pair's entry).
    let _ = app.projection_caches.dual_device_series(
        &app.shell,
        DualDeviceSeriesFamily::NetworkRxTx,
        &disk_id,
        generation,
        load,
    );
    assert_eq!(loads.get(), 2);

    // A real accepted write moves the history epoch: the disk pair reloads.
    taskmanager_shell::fixture::record_demo_history_frame(&mut app.shell, &snapshot, None, None);
    let after_write = app.projection_caches.dual_device_series(
        &app.shell,
        DualDeviceSeriesFamily::DiskReadWrite,
        &disk_id,
        generation,
        load,
    );
    assert_eq!(
        loads.get(),
        3,
        "an accepted history write must reload the pair"
    );
    assert_eq!(after_write.0.as_ref(), [1.0, 2.0]);

    // A visible-capacity change also moves the epoch (the read tail changes).
    app.shell.history.set_capacity(32);
    let _ = app.projection_caches.dual_device_series(
        &app.shell,
        DualDeviceSeriesFamily::DiskReadWrite,
        &disk_id,
        generation,
        load,
    );
    assert_eq!(loads.get(), 4, "a capacity change must reload the pair");

    // A viewed-generation change is a different key even inside the same
    // epoch: the previous instance's cached pair must not survive a
    // row/ring generation flip.
    let _ = app.projection_caches.dual_device_series(
        &app.shell,
        DualDeviceSeriesFamily::DiskReadWrite,
        &disk_id,
        generation + 1,
        load,
    );
    assert_eq!(loads.get(), 5, "a generation change must reload the pair");
}

/// The wired IcedApp accessors serve the split windows the store recorded:
/// the read/write and rx/tx pairs come from the same accepted demo frames as
/// the summed lane, so per-index read + write equals the summed sample — the
/// one-fact-one-authority derivation, observed through the real cache path.
#[test]
fn cached_split_windows_derive_from_the_same_frames_as_the_summed_lane() {
    let mut app = crate::IcedApp::default();
    let demo = taskmanager_shell::demo_app();
    let snapshot = demo
        .projection()
        .snapshot
        .clone()
        .expect("demo snapshot fixture");
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(snapshot.clone()))),
    );
    for _ in 0..3 {
        taskmanager_shell::fixture::record_demo_history_frame(
            &mut app.shell,
            &snapshot,
            None,
            None,
        );
    }

    let disk = &snapshot.disks[0];
    let (read, write) = app.cached_disk_split_series(&disk.device_id, disk.device_generation.get());
    let summed = app.cached_disk_series(&disk.device_id, disk.device_generation.get());
    assert_eq!(read.len(), summed.len());
    assert_eq!(write.len(), summed.len());
    assert!(
        summed.len() >= 2,
        "three demo frames must cross the plotting floor"
    );
    for index in 0..summed.len() {
        assert_eq!(read[index] + write[index], summed[index]);
    }

    let nic = &snapshot.networks[0];
    let (rx, tx) = app.cached_network_split_series(&nic.device_id, nic.device_generation.get());
    let summed_net = app.cached_network_series(&nic.device_id, nic.device_generation.get());
    assert_eq!(rx.len(), summed_net.len());
    assert_eq!(tx.len(), summed_net.len());
    for index in 0..summed_net.len() {
        assert_eq!(rx[index] + tx[index], summed_net[index]);
    }
}
