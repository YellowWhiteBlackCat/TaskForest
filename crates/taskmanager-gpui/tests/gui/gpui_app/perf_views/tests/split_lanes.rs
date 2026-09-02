//! Dual-lane (read/write, rx/tx) render regressions for the Disk and
//! Network main graphs, split from the shared perf-views suite to keep
//! every module under the line guard.

use super::*;

// ── two-series main graphs (disk read/write, NIC rx/tx) ────────────────────

use std::collections::BTreeMap;
use taskmanager_core::core::{
    DeviceLifecycle, DevicePresence, DiskScalarObservations, NetworkScalarObservations,
    NetworkTelemetryObservation, StorageTelemetryObservation,
};

fn split_storage_observation(
    device_id: &str,
    read_bytes_per_sec: u64,
    write_bytes_per_sec: u64,
) -> StorageTelemetryObservation {
    let disk = taskmanager_test_support::DiskMetricsFixtureBuilder::new()
        .device_id(device_id.to_owned())
        .device_generation(DeviceGeneration::new(1))
        .device_state(DeviceState::healthy(10))
        .scalar_observations(DiskScalarObservations {
            read_bytes_per_sec: ScalarObservation::available(read_bytes_per_sec, 10),
            write_bytes_per_sec: ScalarObservation::available(write_bytes_per_sec, 10),
            ..Default::default()
        })
        .build();
    StorageTelemetryObservation::current(
        vec![disk],
        10,
        Vec::new(),
        Vec::new(),
        BTreeMap::from([(
            DeviceId::new(device_id),
            DeviceLifecycle {
                presence: DevicePresence::Present,
                state: DeviceState::healthy(10),
                generation: DeviceGeneration::INITIAL,
                first_seen_ms: Some(10),
                last_seen_ms: Some(10),
                absent_since_ms: None,
            },
        )]),
    )
}

fn split_network_observation(
    device_id: &str,
    rx_bytes_per_sec: u64,
    tx_bytes_per_sec: u64,
) -> NetworkTelemetryObservation {
    let network = taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
        .device_id(std::sync::Arc::from(device_id))
        .device_generation(DeviceGeneration::new(1))
        .device_state(DeviceState::healthy(10))
        .scalar_observations(NetworkScalarObservations {
            rx_bytes_per_sec: ScalarObservation::available(rx_bytes_per_sec, 10),
            tx_bytes_per_sec: ScalarObservation::available(tx_bytes_per_sec, 10),
            ..Default::default()
        })
        .build();
    NetworkTelemetryObservation::current(
        vec![network],
        10,
        Vec::new(),
        Vec::new(),
        BTreeMap::from([(
            DeviceId::new(device_id),
            DeviceLifecycle {
                presence: DevicePresence::Present,
                state: DeviceState::healthy(10),
                generation: DeviceGeneration::INITIAL,
                first_seen_ms: Some(10),
                last_seen_ms: Some(10),
                absent_since_ms: None,
            },
        )]),
    )
}

/// The disk and network main graphs consume the telemetry store's
/// split-direction lanes: with accepted read/write and rx/tx observations the
/// page paints the shared main graph under a two-entry legend whose labels
/// are real localized product keys (not i18n fallbacks), and the split
/// windows the legend names are exactly the store's per-direction evidence.
#[gpui::test]
async fn disk_and_network_pages_paint_two_series_legends_from_split_lanes(cx: &mut TestAppContext) {
    for key in ["disk.read", "disk.write", "net.receive", "net.send"] {
        assert_ne!(
            taskmanager_application::i18n::t(key),
            key,
            "the legend direction labels must be localized product keys"
        );
    }
    let (win, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Performance;
        let stamp = CorrelatedTelemetryStamp::from_accepted_event(1, 10)
            .expect("fixture revision is non-zero");
        v.telemetry_ingestor
            .ingest_correlated_storage(
                stamp,
                &split_storage_observation("disk:wwid:legend", 2_000_000, 500_000),
            )
            .expect("storage split fixture enters system history");
        v.telemetry_ingestor
            .ingest_correlated_network(
                stamp,
                &split_network_observation("net:mac:legend", 3_000_000, 6_000_000),
            )
            .expect("network split fixture enters system history");
        v.system_snapshot_mut_for_test().disks = vec![
            taskmanager_test_support::DiskMetricsFixtureBuilder::new()
                .device_id("disk:wwid:legend".into())
                .device_generation(DeviceGeneration::new(1))
                .device_state(DeviceState::healthy(10))
                .name("legend0n1".into())
                .disk_type("NVMe SSD".into())
                .fs_type("ext4".into())
                .build(),
        ];
        v.system_snapshot_mut_for_test().networks = vec![
            taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
                .device_id("net:mac:legend".into())
                .device_generation(DeviceGeneration::new(1))
                .interface_name("legend0".into())
                .build(),
        ];
        v.selected = SelectedDevice::Disk(0);
        cx.notify();
    });
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    assert!(
        vcx.debug_bounds("tm-graph:main-graph").is_some(),
        "the disk main graph must stay the page's central surface"
    );
    assert!(
        vcx.debug_bounds("tm-graph-legend").is_some(),
        "the disk page must name its two directions with a legend"
    );
    for swatch in ["tm-graph-legend-swatch:0", "tm-graph-legend-swatch:1"] {
        assert!(
            vcx.debug_bounds(swatch).is_some(),
            "each disk direction must own its legend swatch ({swatch})"
        );
    }
    drop(vcx);

    view.update(cx, |v, cx| {
        v.selected = SelectedDevice::Nic(0);
        cx.notify();
    });
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    assert!(
        vcx.debug_bounds("tm-graph-legend").is_some(),
        "the network page must name its two directions with a legend"
    );
    assert!(
        vcx.debug_bounds("tm-graph-legend-swatch:1").is_some(),
        "the transmit direction must keep its own swatch"
    );
    drop(vcx);

    // The windows the legends name are the store's per-direction evidence,
    // not a re-derivation of the summed lane: read/write and rx/tx each hold
    // their own single measured point in MB/s coordinates.
    view.read_with(cx, |v, _| {
        use crate::gpui_app::history_samples::{
            network_rx_rate_samples, network_tx_rate_samples, storage_read_rate_samples,
            storage_write_rate_samples,
        };
        let generation = DeviceGeneration::new(1);
        let system = &v.telemetry.system_history;
        assert_eq!(
            &*storage_read_rate_samples(&v.graph_cache, system, "disk:wwid:legend", generation),
            &[2.0][..]
        );
        assert_eq!(
            &*storage_write_rate_samples(&v.graph_cache, system, "disk:wwid:legend", generation),
            &[0.5][..]
        );
        assert_eq!(
            &*network_rx_rate_samples(&v.graph_cache, system, "net:mac:legend", generation),
            &[3.0][..]
        );
        assert_eq!(
            &*network_tx_rate_samples(&v.graph_cache, system, "net:mac:legend", generation),
            &[6.0][..]
        );
    });
}
