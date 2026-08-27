//! Unit-preference render tests: flipping the applied `app.prefs.units` pair
//! for a device family must change the Performance device texts (bytes/bits ×
//! base-2/base-10), while fixed capacities stay byte-counted (GPUI parity:
//! rates honor units, fixed sizes do not).

use super::frame_text;
use taskmanager_application::{
    MemoryCompositionObservations, MemoryOptionalObservations, MemoryScalarObservations,
    OptionalObservation, ScalarObservation,
};

/// Re-seed the demo memory to whole GiB values so the rendered byte and bit
/// counts are exact (16 total, 4 used, swap 4 total / 1 used).
fn seed_memory(app: &mut crate::TuiApp) {
    app.perf_device = crate::PerfDevice::Memory;
    let gib = 1024_u64 * 1024 * 1024;
    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        let memory = &mut snapshot.as_mut().expect("demo snapshot").memory;
        memory.apply_observations(
            MemoryScalarObservations {
                total_bytes: ScalarObservation::available(16 * gib, 1),
                used_bytes: ScalarObservation::available(4 * gib, 1),
                swap_total_bytes: ScalarObservation::available(4 * gib, 1),
                swap_used_bytes: ScalarObservation::available(gib, 1),
                ..Default::default()
            },
            MemoryOptionalObservations {
                composition: MemoryCompositionObservations {
                    buffers_bytes: OptionalObservation::present(gib / 2, 1),
                    active_bytes: OptionalObservation::present(4 * gib, 1),
                    inactive_bytes: OptionalObservation::present(2 * gib, 1),
                    free_bytes: OptionalObservation::present(8 * gib, 1),
                    reclaimable_bytes: OptionalObservation::present(gib, 1),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
    });
}

/// The applied memory unit preference must reach every formatted text on the
/// Memory view: the in-use/total header, each legend segment, and the swap
/// used/total label all flip between byte and bit counts.
#[test]
fn memory_units_switch_every_composition_text_between_bytes_and_bits() {
    let mut app = crate::demo_app();
    seed_memory(&mut app);

    // Applied default: bytes + base-2.
    let bytes_text = frame_text(&app, 120, 40);
    assert!(
        bytes_text.contains("In use 4.0 GiB"),
        "bytes header must render:\n{bytes_text}"
    );
    assert!(
        bytes_text.contains("16.0 GiB total"),
        "bytes total must render:\n{bytes_text}"
    );
    assert!(
        bytes_text.contains("1.0 GiB / 4.0 GiB"),
        "bytes swap pair must render:\n{bytes_text}"
    );

    // Bits + base-2: every memory text is the 8× value on the binary ladder.
    app.prefs.units[0] = false;
    let bits_text = frame_text(&app, 120, 40);
    assert!(
        bits_text.contains("In use 32.0 Gib"),
        "bits header must render:\n{bits_text}"
    );
    assert!(
        bits_text.contains("128.0 Gib total"),
        "bits total must render:\n{bits_text}"
    );
    assert!(
        bits_text.contains("8.0 Gib / 32.0 Gib"),
        "bits swap pair must render:\n{bits_text}"
    );
    assert!(
        !bits_text.contains("GiB"),
        "no byte unit may remain on the Memory view in bits mode:\n{bits_text}"
    );
}

/// Disk rates honor the applied unit pair while the fixed capacity and free
/// space stay byte-counted (GPUI parity: rates honor units, fixed sizes do
/// not).
#[test]
fn disk_rates_honor_units_while_capacity_stays_bytes() {
    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Disk;

    // Applied default: bytes + base-2.
    let bytes_text = frame_text(&app, 140, 48);
    assert!(
        bytes_text.contains("Read 84.0 MiB/s"),
        "bytes read rate must render:\n{bytes_text}"
    );
    assert!(
        bytes_text.contains("Write 31.0 MiB/s"),
        "bytes write rate must render:\n{bytes_text}"
    );
    assert!(
        bytes_text.contains("2000.0 GiB"),
        "fixed capacity must render in bytes:\n{bytes_text}"
    );

    // Bits + base-2 rates; the fixed capacity must not follow.
    app.prefs.units[2] = false;
    let bits_text = frame_text(&app, 140, 48);
    assert!(
        bits_text.contains("Read 672.0 Mib/s"),
        "bits read rate must render:\n{bits_text}"
    );
    assert!(
        bits_text.contains("Write 248.0 Mib/s"),
        "bits write rate must render:\n{bits_text}"
    );
    assert!(
        bits_text.contains("2000.0 GiB"),
        "fixed capacity must never honor the rate unit:\n{bits_text}"
    );
}

/// Network rates and cumulative totals both honor the applied unit pair —
/// including the shared-config default of bits/base-10.
#[test]
fn network_rates_and_totals_honor_units() {
    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Network;
    let gib = 1024_u64 * 1024 * 1024;
    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        let network = snapshot
            .as_mut()
            .expect("demo app should carry a snapshot")
            .networks
            .first_mut()
            .expect("demo app should carry one NIC");
        let mut observations = *network.scalar_observations();
        observations.total_rx_bytes = ScalarObservation::available(2 * gib, 1);
        observations.total_tx_bytes = ScalarObservation::available(512 * 1024 * 1024, 1);
        let adapter_type = network.adapter_type();
        let wireless = network.wireless_observations().clone();
        network.apply_observations(adapter_type, observations, wireless);
    });

    // Applied default: bits + base-10 (the shared-config network default).
    let bits_text = frame_text(&app, 140, 48);
    assert!(
        bits_text.contains("100.7 Mb/s"),
        "bits rx rate must render:\n{bits_text}"
    );
    assert!(
        bits_text.contains("16.8 Mb/s"),
        "bits tx rate must render:\n{bits_text}"
    );
    assert!(
        bits_text.contains("17.2 Gb"),
        "bits total rx must render:\n{bits_text}"
    );
    assert!(
        bits_text.contains("4.3 Gb"),
        "bits total tx must render:\n{bits_text}"
    );

    // Bytes + base-2: the same rows render byte counts.
    app.prefs.units[4] = true;
    app.prefs.units[5] = true;
    let bytes_text = frame_text(&app, 140, 48);
    assert!(
        bytes_text.contains("12.0 MiB/s"),
        "bytes rx rate must render:\n{bytes_text}"
    );
    assert!(
        bytes_text.contains("2.0 MiB/s"),
        "bytes tx rate must render:\n{bytes_text}"
    );
    assert!(
        bytes_text.contains("2.0 GiB"),
        "bytes total rx must render:\n{bytes_text}"
    );
    assert!(
        bytes_text.contains("512.0 MiB"),
        "bytes total tx must render:\n{bytes_text}"
    );
}
