//! Device + Performance-page render tests: CPU/memory/GPU/disk/network/
//! battery/fan/system sections, the resource selector, and the history
//! graph. Extracted from `ui/tests.rs` to respect the source line budget.

use taskmanager_application::{AppAction, AppPage};
use taskmanager_core::core::device_state::{DeviceState, DeviceStatus};
use taskmanager_core::core::metrics::{
    GpuGraphicsApi, GpuScalarObservations, GpuThrottleReason, OptionalObservation,
    ScalarObservation,
};
use taskmanager_test_support::MemoryMetricsFixtureBuilder;

use super::frame_text;

#[test]
fn every_page_renders_headlessly_at_reference_and_minimum_sizes() {
    for page in [
        AppPage::Performance,
        AppPage::Applications,
        AppPage::Services,
        AppPage::System,
        AppPage::Startup,
        AppPage::Users,
        AppPage::AppHistory,
    ] {
        let mut app = crate::demo_app();
        let _ = app.apply_action(AppAction::SelectPage(page));
        let wide = frame_text(&app, 120, 36);
        assert!(wide.contains("TaskForest"));
        let minimum = frame_text(&app, 54, 16);
        assert!(
            minimum.contains("TF") || minimum.contains("TaskForest"),
            "the narrow header must keep a compact product identity"
        );
    }
}

#[test]
fn process_confirmation_is_visible_and_too_small_state_is_explicit() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    let _ = app.apply_action(AppAction::RequestEndTask);
    let confirmation = frame_text(&app, 100, 30);
    assert!(confirmation.contains("Confirm process action"));
    assert!(confirmation.contains("re-check PID"));

    let small = frame_text(&app, 40, 10);
    assert!(small.contains("Terminal too small"));
}

#[test]
fn cpu_view_renders_all_facts_without_fabricated_zeroes() {
    let mut app = crate::demo_app();
    // Cold CPU frequency/temperature observations render dashes instead of
    // fabricated numeric values.
    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        let snapshot = snapshot.as_mut().expect("demo snapshot");
        let mut observations = snapshot.cpu.scalar_observations().clone();
        observations.frequency_mhz = ScalarObservation::default();
        observations.temperature_c = ScalarObservation::default();
        snapshot.cpu.apply_scalar_observations(observations);
    });

    let text = frame_text(&app, 120, 36);

    // The CPU fact strip carries the live reading.
    assert!(text.contains("37.4%"));
    // No fabricated zero units may appear for the cold cpu fields.
    assert!(!text.contains("0 MHz"));
    assert!(!text.contains("0°C"));
    // The selector tab row is present with the default Cpu entry.
    assert!(text.contains("CPU"));
}

#[test]
fn performance_selector_switch_changes_the_rendered_detail() {
    let mut app = crate::demo_app();
    // Default CPU overview: all available CPU histories share the page. The
    // GPU brand lives only in the dedicated GPU panel.
    let cpu_text = frame_text(&app, 120, 36);
    assert!(
        cpu_text.contains("CPU Utilization (%)")
            && cpu_text.contains("Temperature 54°C")
            && cpu_text.contains("Frequency 3284 MHz"),
        "the default CPU view must pair one utilization graph with every current fact"
    );
    assert!(
        !cpu_text.contains("Intel Graphics (xe)"),
        "the GPU brand must be absent from the Cpu overview"
    );

    // Selecting GPU reuses the dedicated panel: brand appears, CPU histories
    // are gone.
    app.select_perf_device(crate::PerfDevice::Gpu);
    let gpu_text = frame_text(&app, 120, 36);
    assert!(
        gpu_text.contains("Intel Graphics (xe)"),
        "the Gpu view must render the dedicated GPU panel"
    );
    assert!(
        !gpu_text.contains("CPU Utilization (%)"),
        "CPU histories must be gone once the GPU resource is selected"
    );
}

#[test]
fn performance_selector_renders_resource_tab_row_with_active_highlight() {
    let app = crate::demo_app();
    // The compact selector row lists every resource with its digit shortcut.
    let text = frame_text(&app, 120, 36);
    assert!(text.contains("CPU"));
    assert!(text.contains("Memory"));
    assert!(text.contains("Disk"));
    assert!(text.contains("Network"));
    assert!(text.contains("GPU"));
    assert!(text.contains("Fan"));
    assert!(text.contains("1-7 select"));
}

#[test]
fn missing_memory_denominators_render_as_dashes_instead_of_zero_percent() {
    let mut app = crate::demo_app();
    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        snapshot.as_mut().expect("demo snapshot").memory = MemoryMetricsFixtureBuilder::new()
            .current_total_bytes(0)
            .current_used_bytes(0)
            .current_swap_total_bytes(0)
            .current_swap_used_bytes(0)
            .build();
    });

    let text = frame_text(&app, 120, 36);

    assert!(text.contains('—'));
    assert!(!text.contains("0.0%"));
}

/// The Memory Performance view renders the shared composition bar: a
/// header, the stacked proportion bar, a per-category legend, and the
/// secondary swap bar. The breakdown routes through the shared shell
/// module (single-source), so a fully-measured memory snapshot yields the
/// five active/inactive/cache/free/other categories.
#[test]
fn memory_view_renders_composition_bar_with_categories_and_swap() {
    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Memory;
    let gib = 1024_u64 * 1024 * 1024;
    let mut snapshot = app.projection().snapshot.clone().expect("demo snapshot");
    snapshot.memory = MemoryMetricsFixtureBuilder::new()
        .current_total_bytes(16 * gib)
        .current_used_bytes(4 * gib)
        .current_swap_total_bytes(4 * gib)
        .current_swap_used_bytes(gib)
        .buffers_bytes(gib / 2)
        .active_bytes(4 * gib)
        .inactive_bytes(2 * gib)
        .free_bytes(8 * gib)
        .reclaimable_bytes(gib)
        .build();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
    );

    let text = frame_text(&app, 120, 40);

    // Header + every measured category from the five-segment path.
    assert!(text.contains("Composition"));
    assert!(text.contains("Active"));
    assert!(text.contains("Inactive"));
    assert!(text.contains("Cache + Buffers"));
    assert!(text.contains("Free"));
    // The secondary swap bar carries its used/total label.
    assert!(text.contains("Swap"));
}

/// The swap bar carries the full zram `mm_stat` depth: the swap-used view,
/// the RAM the store actually consumes (`mem_used_total`, metadata
/// included), and the guarded compression readout — the same facts the
/// iced/gpui swap readouts label, so no frontend hides the RAM cost.
#[test]
fn swap_bar_labels_the_zram_ram_used_from_mm_stat() {
    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Memory;
    let mib = 1024_u64 * 1024;
    let mut snapshot = app.projection().snapshot.clone().expect("demo snapshot");
    // Keep the demo composition (the composition bar gates the whole memory
    // view) and layer only the zram depth onto it.
    snapshot.memory = MemoryMetricsFixtureBuilder::from_item(snapshot.memory.clone())
        .current_swap_total_bytes(4 * mib)
        .current_swap_used_bytes(mib)
        .compressed_swap_used_bytes(mib)
        .compressed_swap_capacity_bytes(4 * mib)
        .compressed_swap_original_bytes(3 * mib)
        .compressed_swap_compressed_bytes(mib)
        .compressed_swap_memory_used_bytes(mib / 2)
        .build();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
    );

    let text = frame_text(&app, 160, 40);

    assert!(
        text.contains("zram swap"),
        "the swap-used view of the zram store must render:\n{text}"
    );
    assert!(
        text.contains("zram RAM used"),
        "mm_stat mem_used_total must not hide behind the swap-used view:\n{text}"
    );
    assert!(
        text.contains("Compression ratio 3.0:1"),
        "the guarded compression depth must render:\n{text}"
    );
}

/// The composition bar is Memory-view only: the default Cpu view keeps the
/// gauges + history graph and does not render the composition categories.
#[test]
fn cpu_view_omits_the_memory_composition_bar() {
    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Cpu;
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Performance));
    let text = frame_text(&app, 120, 40);
    assert!(!text.contains("Cache + Buffers"));
    assert!(!text.contains("Other / Reserved"));
}

#[test]
fn missing_gpu_observations_render_as_dashes() {
    let mut app = crate::demo_app();
    // The dedicated GPU panel only renders under the Gpu selector.
    app.perf_device = crate::PerfDevice::Gpu;
    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        let gpu = snapshot
            .as_mut()
            .and_then(|snapshot| snapshot.gpu.first_mut())
            .expect("demo app should carry one GPU");
        gpu.apply_scalar_observations(GpuScalarObservations::default());
    });

    let text = frame_text(&app, 120, 36);

    // The Gpu panel header renders brand + utilization + temperature, each
    // from the typed accessors, so every missing observation is a dash.
    assert!(
        text.contains("Intel Graphics (xe) · Utilization — · Temperature —"),
        "typed unavailable GPU facts must remain explicit:\n{text}"
    );
    assert!(!text.contains("0 MHz"));
    assert!(!text.contains("0°C"));
}

#[test]
fn gpu_detail_section_renders_utilization_vram_clocks_and_engines() {
    use taskmanager_core::core::metrics::{GpuEngine, GpuEngineKind};

    let mut app = crate::demo_app();
    // The dedicated GPU panel only renders under the Gpu selector.
    app.perf_device = crate::PerfDevice::Gpu;
    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        let gpu = snapshot
            .as_mut()
            .and_then(|snapshot| snapshot.gpu.first_mut())
            .expect("demo app should carry one GPU");
        // Populate every field the dedicated GPU panel reads through canonical
        // observations.
        gpu.apply_scalar_observations(GpuScalarObservations {
            utilization_pct: ScalarObservation::available(73.0, 1),
            temperature_c: ScalarObservation::available(64.0, 1),
            dedicated_vram_used_bytes: ScalarObservation::available(1 << 30, 1),
            dedicated_vram_total_bytes: ScalarObservation::available(8 << 30, 1),
            max_frequency_mhz: ScalarObservation::available(2100, 1),
            idle_residency_pct: ScalarObservation::available(78.0, 1),
            power_w: ScalarObservation::available(8.4, 1),
            shared_vram_used_bytes: ScalarObservation::available(512 << 20, 1),
            shared_vram_total_bytes: ScalarObservation::available(2 << 30, 1),
            ..Default::default()
        });
        gpu.engines.push(GpuEngine {
            name: "Render/3D".into(),
            kind: GpuEngineKind::Render,
            usage_pct: 42.0,
        });
        // The split-VRAM + telemetry rows the panel gained for Mission Center
        // parity: idle residency, power draw, driver, the shared VRAM aperture
        // (shown alongside dedicated) and a throttling reason.
        gpu.driver = Some("xe".into());
        gpu.apply_throttle_observation(ScalarObservation::available(
            vec![GpuThrottleReason::HardwareThermalLimit],
            1,
        ));
        gpu.marketing_name = Some("Arc B390".into());
    });

    let text = frame_text(&app, 120, 48);

    // The dedicated GPU panel renders fields the one-line summary never
    // shows, so each assertion proves the panel itself drew real data.
    // Per-engine utilization is only ever rendered by this panel.
    assert!(
        text.contains("Render/3D"),
        "per-engine name must render in the GPU panel"
    );
    assert!(text.contains("42.0%"), "per-engine utilization must render");
    // Dedicated VRAM pair (binary-unit formatted via the shared helper).
    assert!(text.contains("1.0 GiB"));
    assert!(text.contains("8.0 GiB"));
    // Max clock is panel-only; the summary line carries only the live clock.
    assert!(
        text.contains("2100 MHz"),
        "max clock must render in the GPU panel"
    );
    assert!(
        text.contains("Arc B390"),
        "PCI marketing name must render in the GPU panel"
    );
    // Aggregate utilization from the typed accessor.
    assert!(text.contains("73.0%"));
    // RC6 idle residency, power draw, driver, the shared VRAM aperture
    // (shown alongside dedicated) and the throttling reason are panel rows
    // the one-line summary never renders.
    assert!(text.contains("78.0%"), "idle residency must render");
    assert!(text.contains("8.4 W"), "power draw must render");
    assert!(text.contains("xe"), "driver must render");
    assert!(
        text.contains("512.0 MiB"),
        "shared VRAM used must render alongside dedicated"
    );
    assert!(
        text.contains("2.0 GiB"),
        "shared VRAM total must render alongside dedicated"
    );
    assert!(text.contains("thermal"), "throttling reason must render");
}

#[test]
fn gpu_detail_section_renders_honest_empty_state_when_no_gpu() {
    let mut app = crate::demo_app();
    // The dedicated GPU panel only renders under the Gpu selector.
    app.perf_device = crate::PerfDevice::Gpu;
    // Strip every GPU so the panel cannot fall back to a fabricated idle
    // reading; it must say so honestly.
    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        snapshot.as_mut().expect("demo snapshot").gpu.clear();
    });

    let text = frame_text(&app, 120, 36);

    // The dedicated panel's empty message is distinct from the summary's
    // "No GPU telemetry" one-liner (it adds "available"), so this proves
    // the panel itself rendered its honest empty state.
    assert!(
        text.contains("No GPU telemetry available"),
        "the GPU panel must render its honest empty state"
    );
    // No fabricated GPU numbers may appear: there is no GPU to read.
    assert!(
        !text.contains("Intel Graphics"),
        "no GPU brand may render when the vector is empty"
    );
}

#[test]
fn disk_detail_section_renders_rates_smart_and_partition_space() {
    let mut app = crate::demo_app();
    // The dedicated disk panel only renders under the Disk selector.
    app.perf_device = crate::PerfDevice::Disk;
    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        let snapshot = snapshot.as_mut().expect("demo snapshot");
        let gib = 1024_u64.pow(3);
        // SMART health fields the compact devices summary never shows.
        let disk = snapshot
            .disks
            .first_mut()
            .expect("demo app should carry one disk");
        disk.smart_temperature_c = Some(42.0);
        disk.smart_percent_used = Some(5.0);
        // Latency/throughput, top-level capacity, filesystem and SMART-depth
        // rows the panel gained for Mission Center parity.
        let mut disk_observations = *disk.scalar_observations();
        disk_observations.response_time_ms = ScalarObservation::available(1.5, 1);
        disk_observations.iops = ScalarObservation::available(137, 1);
        disk.apply_scalar_observations(disk_observations);
        disk.fs_type = "btrfs".into();
        disk.smart_temp_critical_c = Some(70.0);
        disk.smart_power_on_hours = Some(8800);
        // One mounted partition child, built through Default + public fields so
        // the test never names the (non-re-exported) DiskPartition type.
        disk.partitions.push(Default::default());
        let partition = disk
            .partitions
            .last_mut()
            .expect("pushed partition is present");
        partition.name = "nvme0n1p1".into();
        partition.mount_point = "/".into();
        partition.apply_scalar_observations(
            taskmanager_core::core::metrics::DiskPartitionScalarObservations {
                capacity_bytes: ScalarObservation::available(500 * gib, 1),
                free_bytes: ScalarObservation::available(200 * gib, 1),
                ..Default::default()
            },
        );
    });

    let text = frame_text(&app, 140, 48);

    // Every assertion is a value the compact one-line devices summary never
    // renders, so each proves the dedicated disk panel itself drew real data.
    assert!(text.contains("nvme0n1"), "disk name must render");
    assert!(text.contains("84.0 MiB"), "read rate must render");
    assert!(text.contains("31.0 MiB"), "write rate must render");
    assert!(text.contains("12.7%"), "active time must render");
    assert!(
        text.contains("42 / 70°C"),
        "SMART temperature with critical must render"
    );
    assert!(text.contains("5.0%"), "SMART endurance used must render");
    // Partition space is panel-only: name + capacity + free.
    assert!(text.contains("nvme0n1p1"), "partition name must render");
    assert!(text.contains("500.0 GiB"), "partition capacity must render");
    assert!(
        text.contains("200.0 GiB"),
        "partition free space must render"
    );
    // Latency/throughput, top-level capacity, filesystem and SMART-depth
    // rows the panel gained for parity with the GPUI disk_stats view.
    assert!(text.contains("1.50 ms"), "response time must render");
    assert!(text.contains("137"), "IOPS must render");
    assert!(text.contains("btrfs"), "filesystem type must render");
    assert!(
        text.contains("2000.0 GiB"),
        "top-level disk capacity must render"
    );
    assert!(
        text.contains("1240.0 GiB"),
        "top-level disk free space must render"
    );
    assert!(text.contains("8800 h"), "power-on hours must render");
}

#[test]
fn network_detail_section_renders_rates_link_and_wireless_association() {
    let mut app = crate::demo_app();
    // The dedicated network panel only renders under the Network selector.
    app.perf_device = crate::PerfDevice::Network;
    // The applied default network units are bits/base-10 (shared-config
    // default), so pin the bytes/base-2 pair to make the byte-count
    // assertions below explicit rather than default-dependent.
    app.prefs.units[4] = true;
    app.prefs.units[5] = true;
    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        let snapshot = snapshot.as_mut().expect("demo snapshot");
        // Populate the typed wireless/link fields the panel reads.
        let network = snapshot
            .networks
            .first_mut()
            .expect("demo app should carry one NIC");
        let mut scalar_observations = *network.scalar_observations();
        scalar_observations.utilization_pct = ScalarObservation::available(8.0, 1);
        scalar_observations.link_speed_mbps = ScalarObservation::available(1200, 1);
        let mut wireless_observations = network.wireless_observations().clone();
        wireless_observations.signal_dbm = OptionalObservation::present(-52, 1);
        network.apply_observations(
            taskmanager_core::core::metrics::NetworkAdapterType::WiFi,
            scalar_observations,
            wireless_observations,
        );
        // Address, MAC and native-identity rows the panel gained for parity with
        // the GPUI network_stats view.
        network.ipv6_addr = Some("fe80::1".into());
        network.mac_addr = Some("aa:bb:cc:dd:ee:ff".into());
        network.driver = Some("iwlwifi".into());
        network.adapter = Some("Intel Wi-Fi".into());
    });

    let text = frame_text(&app, 140, 48);

    assert!(text.contains("wlan0"), "interface name must render");
    assert!(
        text.contains("Wireless"),
        "wireless adapter type must render"
    );
    assert!(text.contains("12.0 MiB"), "rx rate must render");
    assert!(text.contains("2.0 MiB"), "tx rate must render");
    assert!(text.contains("8.0%"), "utilization must render");
    assert!(text.contains("1200 Mbps"), "link speed must render");
    assert!(text.contains("Connected"), "connection verdict must render");
    // Wireless association is panel-only.
    assert!(text.contains("TaskForest Lab"), "SSID must render");
    assert!(text.contains("-52 dBm"), "signal level must render");
    // Address/MAC + native identity rows.
    assert!(text.contains("192.168.1.42"), "IPv4 address must render");
    assert!(text.contains("fe80::1"), "IPv6 address must render");
    assert!(
        text.contains("aa:bb:cc:dd:ee:ff"),
        "MAC address must render"
    );
    assert!(text.contains("iwlwifi"), "driver must render");
    assert!(text.contains("Intel Wi-Fi"), "adapter must render");
}

#[test]
fn system_page_renders_the_full_hardware_and_telemetry_fact_set() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::System));
    let mut hardware = app.projection().hardware.clone().expect("demo hardware");
    hardware.displays = vec![taskmanager_core::core::hardware::DisplayInfo {
        connector: "DP-1".into(),
        manufacturer: Some("DEL".into()),
        model: Some("TaskPanel".into()),
        width_px: Some(1920),
        height_px: Some(1080),
        refresh_hz: Some(60.0),
        hdr_supported: Some(true),
        ..Default::default()
    }];
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Hardware(Some(Box::new(hardware))),
    );
    let first = frame_text(&app, 120, 52);
    app.system_scroll = usize::MAX;
    let tail = frame_text(&app, 120, 52);
    let text = format!("{first}\n{tail}");
    // Hardware facts populated by the demo fixture.
    assert!(text.contains("Linux"), "OS name must render");
    assert!(text.contains("Arch Linux"), "OS version must render");
    assert!(text.contains("32768 MiB"), "installed memory must render");
    // Telemetry rows the page gained for parity with the GPUI system view.
    assert!(text.contains("347"), "process count must render");
    assert!(text.contains("2816"), "thread count must render");
    assert!(text.contains("DP-1 · DEL TaskPanel · 1920×1080 · 60.0 Hz"));
    assert!(text.contains("HDR"), "HDR state must render");
}

#[test]
fn disk_and_network_selectors_render_honest_empty_state_when_absent() {
    let mut app = crate::demo_app();
    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        let snapshot = snapshot.as_mut().expect("demo snapshot");
        snapshot.disks.clear();
        snapshot.networks.clear();
    });

    // The selector shows one resource at a time, so each empty state is
    // asserted on its own tab render.
    app.perf_device = crate::PerfDevice::Disk;
    let disk_text = frame_text(&app, 140, 48);
    assert!(
        disk_text.contains("No disk telemetry available"),
        "the disk panel must render its honest empty state"
    );
    assert!(
        !disk_text.contains("nvme0n1"),
        "no disk name may render when the vector is empty"
    );

    app.perf_device = crate::PerfDevice::Network;
    let network_text = frame_text(&app, 140, 48);
    assert!(
        network_text.contains("No network telemetry reported"),
        "the network panel must render its honest empty state"
    );
    assert!(
        !network_text.contains("wlan0"),
        "no interface name may render when the vector is empty"
    );
}

#[test]
fn battery_detail_section_renders_capacity_status_rate_and_voltage() {
    use taskmanager_core::core::device_state::DeviceState;
    use taskmanager_core::core::power::{BatteryInfo, PowerSupplySnapshot};

    let mut app = crate::demo_app();
    // The dedicated battery panel only renders under the Battery selector.
    app.perf_device = crate::PerfDevice::Battery;
    let mut charged = BatteryInfo::new("power-supply:BAT0", DeviceState::healthy(1_000));
    charged.status = "Discharging".into();
    charged.technology = "Li-ion".into();
    charged.manufacturer = "TaskForest Cells".into();
    charged.apply_scalar_observations(taskmanager_core::core::power::BatteryScalarObservations {
        capacity_pct: taskmanager_core::core::metrics::ScalarObservation::available(82, 1_000),
        voltage_uv: taskmanager_core::core::metrics::ScalarObservation::available(
            12_400_000, 1_000,
        ),
        power_w: taskmanager_core::core::metrics::ScalarObservation::available(9.5, 1_000),
        cycle_count: taskmanager_core::core::metrics::ScalarObservation::available(318, 1_000),
        ..Default::default()
    });
    let mut cold = BatteryInfo::default();
    cold.status = "Unknown".into();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::PowerSupplies(Some(PowerSupplySnapshot {
            state: DeviceState::healthy(1_000),
            timestamp_ms: 1_000,
            batteries: vec![charged, cold],
            ..Default::default()
        })),
    );

    let text = frame_text(&app, 140, 48);

    // Every assertion is a value only the dedicated battery panel renders,
    // read through the typed accessors so each proves the panel drew real
    // per-battery data rather than a fabricated idle reading.
    assert!(text.contains("82%"), "capacity percent must render");
    assert!(text.contains("Discharging"), "status must render");
    assert!(text.contains("9.5 W"), "charge/discharge rate must render");
    assert!(text.contains("12.40 V"), "voltage must render");
    assert!(text.contains("Li-ion"), "technology descriptor must render");
    assert!(
        text.contains("TaskForest Cells"),
        "manufacturer descriptor must render"
    );
    assert!(text.contains("318"), "cycle count must render");
    // The cold battery's unknown capacity is an honest dash, never 0%. The
    // only literal "0%" in the renderer is the Cpu/Memory history y-axis,
    // which the Battery tab does not draw, so its absence is meaningful.
    assert!(
        !text.contains("0%"),
        "a None capacity must render a dash, never a fabricated 0%"
    );
    // The selector exposes the new resource with its updated digit range.
    assert!(
        text.contains("Battery"),
        "the Battery selector entry must show"
    );
    assert!(
        text.contains("1-7 select"),
        "the digit hint must cover 1..=7"
    );
}

#[test]
fn battery_detail_section_renders_honest_empty_state_when_no_power_snapshot() {
    use taskmanager_core::core::device_state::DeviceState;
    use taskmanager_core::core::power::PowerSupplySnapshot;

    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Battery;

    // No power batch has landed yet (desktop host / first tick): the panel
    // must say so honestly rather than fabricate an idle battery.
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::PowerSupplies(None),
    );
    let none_text = frame_text(&app, 140, 48);
    assert!(
        none_text.contains("No battery or power supply was detected"),
        "the battery panel must render its honest empty state for a None snapshot"
    );
    assert!(
        !none_text.contains("Discharging"),
        "no battery status may render when the snapshot is absent"
    );

    // A snapshot that arrived with zero batteries is the same honest empty
    // state, not a fabricated idle board.
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::PowerSupplies(Some(PowerSupplySnapshot {
            state: DeviceState::healthy(1_000),
            timestamp_ms: 1_000,
            batteries: Vec::new(),
            ..Default::default()
        })),
    );
    let empty_text = frame_text(&app, 140, 48);
    assert!(
        empty_text.contains("No battery or power supply was detected"),
        "the battery panel must render its honest empty state for an empty vector"
    );
    assert!(
        !empty_text.contains("82%"),
        "no capacity may render when no battery is present"
    );
}

#[test]
fn fan_detail_section_renders_rpm_pwm_and_device_temperatures() {
    use taskmanager_core::core::device_state::DeviceState;
    use taskmanager_core::core::sensors::{
        SensorCenterSnapshot, SensorDescriptor, SensorMagnitude, SensorMeasurementObservation,
        SensorReading,
    };

    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Fan;
    // A healthy hwmon board with one fan channel (current RPM + duty cycle)
    // and one temperature channel on the same physical device.
    let fan = SensorReading::from_measurement_observation(
        "hwmon:cpu".into(),
        "fan1".into(),
        "cpu_fan".into(),
        SensorMeasurementObservation::available(
            SensorDescriptor::fan_speed(taskmanager_core::core::sensors::SensorScale::IDENTITY),
            SensorMagnitude::Unsigned(2_400),
            1_000,
        )
        .expect("valid fan magnitude"),
    )
    .with_device_generation(taskmanager_core::core::identity::DeviceGeneration::new(1));
    let pwm = SensorReading::from_measurement_observation(
        "hwmon:cpu".into(),
        "pwm1".into(),
        "fan1_pwm".into(),
        SensorMeasurementObservation::available(
            SensorDescriptor::pwm_duty_cycle(),
            SensorMagnitude::DutyCycle {
                value: 60,
                maximum: 255,
            },
            1_000,
        )
        .expect("valid duty-cycle magnitude"),
    )
    .with_device_generation(taskmanager_core::core::identity::DeviceGeneration::new(1));
    let temperature = SensorReading::from_measurement_observation(
        "hwmon:cpu".into(),
        "temp1".into(),
        "cpu_temp".into(),
        SensorMeasurementObservation::available(
            SensorDescriptor::temperature(taskmanager_core::core::sensors::SensorScale::IDENTITY),
            SensorMagnitude::Decimal(54.5),
            1_000,
        )
        .expect("valid temperature magnitude"),
    )
    .with_device_generation(taskmanager_core::core::identity::DeviceGeneration::new(1));
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Sensors(Some(SensorCenterSnapshot {
            state: DeviceState::healthy(1_000),
            timestamp_ms: 1_000,
            readings: vec![fan, pwm, temperature],
            ..Default::default()
        })),
    );

    let text = frame_text(&app, 140, 48);

    // Every assertion is a value only the dedicated fan panel renders, read
    // through the typed accessors so each proves the panel drew real per-fan
    // data rather than a fabricated idle reading.
    assert!(text.contains("cpu_fan"), "fan label must render");
    assert!(text.contains("2400 RPM"), "fan speed must render");
    assert!(
        text.contains("24%"),
        "duty cycle must render as a percent of its maximum"
    );
    assert!(text.contains("54.5 °C"), "device temperature must render");
}

#[test]
fn fan_detail_section_renders_honest_empty_state_without_sensor_data() {
    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Fan;

    // No sensor batch has landed yet: the panel must say so honestly rather
    // than fabricate an idle fan.
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Sensors(None),
    );
    let none_text = frame_text(&app, 140, 48);
    assert!(
        none_text.contains("No fan sensor was detected"),
        "the fan panel must render its honest empty state for a None snapshot"
    );
    assert!(
        !none_text.contains("RPM"),
        "no speed may render when the sensor snapshot is absent"
    );

    // A snapshot with no fan channels is the same honest empty state.
    use taskmanager_core::core::device_state::DeviceState;
    use taskmanager_core::core::sensors::{
        SensorCenterSnapshot, SensorDescriptor, SensorMagnitude, SensorMeasurementObservation,
        SensorReading, SensorScale,
    };
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Sensors(Some(SensorCenterSnapshot {
            state: DeviceState::healthy(1_000),
            timestamp_ms: 1_000,
            readings: vec![
                SensorReading::from_measurement_observation(
                    "hwmon:cpu".into(),
                    "temp1".into(),
                    "cpu_temp".into(),
                    SensorMeasurementObservation::available(
                        SensorDescriptor::temperature(SensorScale::IDENTITY),
                        SensorMagnitude::Decimal(40.0),
                        1_000,
                    )
                    .expect("valid temperature fixture"),
                )
                .with_device_generation(taskmanager_core::core::identity::DeviceGeneration::new(1)),
            ],
            ..Default::default()
        })),
    );
    let empty_text = frame_text(&app, 140, 48);
    assert!(
        empty_text.contains("No fan sensor was detected"),
        "the fan panel must render its honest empty state for a fanless snapshot"
    );
}

#[test]
fn network_subcategory_visibility_filters_the_nic_panel() {
    use taskmanager_core::core::metrics::NetworkAdapterType;
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Performance));
    app.perf_device = crate::PerfDevice::Network;
    let mut snapshot = app.projection().snapshot.clone().expect("demo snapshot");
    let mut wired = snapshot.networks[0].clone();
    wired.interface_name = "eth0".into();
    let wired_scalars = *wired.scalar_observations();
    wired.apply_observations(
        NetworkAdapterType::Ethernet,
        wired_scalars,
        taskmanager_core::core::metrics::NetworkWirelessObservations::not_applicable(1),
    );
    let mut vpn = snapshot.networks[0].clone();
    vpn.interface_name = "tun0".into();
    let vpn_scalars = *vpn.scalar_observations();
    vpn.apply_observations(
        NetworkAdapterType::Vpn,
        vpn_scalars,
        taskmanager_core::core::metrics::NetworkWirelessObservations::not_applicable(1),
    );
    snapshot.networks = vec![wired, vpn];
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
    );

    // Both NICs render by default.
    let text = frame_text(&app, 120, 40);
    assert!(text.contains("eth0"));
    assert!(text.contains("tun0"));

    // Hiding VPNs removes tun0 but keeps the wired NIC.
    app.prefs.show[6] = false;
    let text = frame_text(&app, 120, 40);
    assert!(text.contains("eth0"));
    assert!(!text.contains("tun0"), "hidden VPN class must drop out");

    // Hiding every class renders the honest empty state, never a fake NIC.
    app.prefs.show[4] = false;
    app.prefs.show[6] = false;
    let text = frame_text(&app, 120, 40);
    assert!(text.contains("No network telemetry reported"));
}

#[test]
fn compact_gpu_frame_keeps_the_fixed_utilization_and_key_facts() {
    let mut app = crate::demo_app();
    app.select_perf_device(crate::PerfDevice::Gpu);
    let compact = frame_text(&app, 54, 16);

    for fact in ["18.0%", "48°C", "900 MHz", "Power —"] {
        assert!(
            compact.contains(fact),
            "compact GPU frame lost primary fact {fact:?}:\n{compact}"
        );
    }
}

#[test]
fn system_npu_facts_are_reachable_at_reference_and_compact_sizes() {
    let expected = [
        "Intel AI Boost",
        "intel_vpu",
        "44.0%",
        "Compute engine",
        "Matrix engine",
        "Vector engine",
        "Video engine",
        "Copy engine",
        "Other engine",
        "512.0 MiB",
        "4.0 GiB",
        "Utilization        44.0%",
        "Compute engine     11.0%",
        "Dedicated memory   512.0 MiB",
    ];

    for (width, height) in [(120, 36), (54, 16)] {
        let mut app = crate::demo_app();
        let _ = app.apply_action(AppAction::SelectPage(AppPage::System));
        let mut visited = String::new();
        for offset in 0..72 {
            app.system_scroll = offset;
            visited.push_str(&frame_text(&app, width, height));
            visited.push('\n');
        }
        for fact in expected {
            assert!(
                visited.contains(fact),
                "{width}×{height} System viewport never reached NPU fact {fact:?}"
            );
        }
    }
}

/// The dedicated GPU panel names every proven graphics-API version and the
/// PCI slot end-to-end (§2.5 B-1..B-3, GPUI gpu_stats parity): the rows ride
/// the fixed full fact strip under the Gpu selector.
#[test]
fn gpu_panel_names_proven_graphics_apis_and_pci_slot() {
    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Gpu;
    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        let gpu = snapshot
            .as_mut()
            .and_then(|snapshot| snapshot.gpu.first_mut())
            .expect("demo app should carry one GPU");
        gpu.graphics_api = Some(GpuGraphicsApi {
            opengl_version: Some("4.6".into()),
            vulkan_version: Some("1.3.290".into()),
        });
        gpu.pci_slot = Some("0000:03:00.0".into());
    });

    let text = frame_text(&app, 120, 48);

    assert!(
        text.contains("OpenGL version 4.6"),
        "the proven OpenGL version must render in the GPU panel:\n{text}"
    );
    assert!(
        text.contains("Vulkan version 1.3.290"),
        "the proven Vulkan version must render in the GPU panel:\n{text}"
    );
    assert!(
        text.contains("PCI slot 0000:03:00.0"),
        "the PCI slot must render in the GPU panel:\n{text}"
    );
}

/// The disk panel carries the typed device-health verdict (§2.3 B-1) and the
/// Removable row for proven removable media (§2.3 B-2); the network panel
/// carries the same health verdict (§2.4 B-1) while its carrier verdict stays
/// independently Connected — the demo NIC keeps its assigned IPv4 address.
#[test]
fn disk_and_network_panels_carry_device_health_and_proven_removability() {
    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Disk;
    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        let disk = snapshot
            .as_mut()
            .expect("demo snapshot")
            .disks
            .first_mut()
            .expect("demo app should carry one disk");
        disk.device_state = DeviceState::healthy(1_000);
        disk.apply_attachment_capabilities(Some(true), None);
    });
    let disk_text = frame_text(&app, 140, 48);
    assert!(
        disk_text.contains("Status Healthy"),
        "the disk panel must render the typed device health:\n{disk_text}"
    );
    assert!(
        disk_text.contains("Removable Yes"),
        "proven removable media must render its row:\n{disk_text}"
    );

    app.perf_device = crate::PerfDevice::Network;
    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        let network = snapshot
            .as_mut()
            .expect("demo snapshot")
            .networks
            .first_mut()
            .expect("demo app should carry one NIC");
        network.device_state = DeviceState {
            status: DeviceStatus::Stale,
            last_success_ms: Some(1_000),
        };
    });
    let network_text = frame_text(&app, 140, 48);
    assert!(
        network_text.contains("Status Stale data"),
        "the NIC panel must express degraded device health:\n{network_text}"
    );
    assert!(
        network_text.contains("Connected"),
        "the carrier verdict stays independent of device health:\n{network_text}"
    );
}

/// The new health/capability rows wrap inside a narrow frame instead of
/// overflowing it: the compact-width draw must keep the short verdict tokens
/// visible (wrapped lines break at spaces, never mid-token). Height 28 keeps
/// the disk detail itself on screen beside the 12-row directory-usage panel
/// the demo projects under the Disk tab.
#[test]
fn narrow_frames_keep_the_health_rows_wrapped_not_dropped() {
    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Disk;
    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        let disk = snapshot
            .as_mut()
            .expect("demo snapshot")
            .disks
            .first_mut()
            .expect("demo app should carry one disk");
        disk.device_state = DeviceState::healthy(1_000);
        disk.apply_attachment_capabilities(Some(true), None);
    });
    let disk_narrow = frame_text(&app, 54, 28);
    assert!(
        disk_narrow.contains("Healthy") && disk_narrow.contains("Yes"),
        "the narrow disk frame must keep the health and removable rows:\n{disk_narrow}"
    );

    app.perf_device = crate::PerfDevice::Network;
    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        let network = snapshot
            .as_mut()
            .expect("demo snapshot")
            .networks
            .first_mut()
            .expect("demo app should carry one NIC");
        network.device_state = DeviceState {
            status: DeviceStatus::Stale,
            last_success_ms: Some(1_000),
        };
    });
    let network_narrow = frame_text(&app, 54, 28);
    assert!(
        network_narrow.contains("Stale data"),
        "the narrow network frame must keep the health row:\n{network_narrow}"
    );
}

/// System page Storage section (§2.8 B-1, GPUI storage_section parity): one
/// static identity/capacity row per discovered disk carrying name,
/// capacity, available and type, reachable by scroll at reference and
/// compact sizes.
#[test]
fn system_page_renders_the_storage_section_from_snapshot_disks() {
    for (width, height) in [(120, 52), (54, 16)] {
        let mut app = crate::demo_app();
        let _ = app.apply_action(AppAction::SelectPage(AppPage::System));
        let mut visited = String::new();
        for offset in 0..80 {
            app.system_scroll = offset;
            visited.push_str(&frame_text(&app, width, height));
            visited.push('\n');
        }
        // Wrapping inflates the row count at narrow widths, so reach the tail
        // explicitly instead of relying on the offset range alone.
        app.system_scroll = usize::MAX;
        visited.push_str(&frame_text(&app, width, height));
        assert!(
            visited.contains("Storage"),
            "{width}×{height} System viewport never reached the storage section"
        );
        // Field quadruple: name, capacity, available, type. At width 120 the
        // whole row paints on one line; at width 54 the row word-wraps and the
        // bounded viewport clips the LAST entry's wrapped continuation (an
        // honest viewport bound, never an overflow), so the narrow pass
        // asserts the visible first chunk instead.
        let facts: &[&str] = if width > 80 {
            &["TiPro9000 2TB · 2000.0 GiB · 1240.0 GiB · NVMe SSD"]
        } else {
            &["TiPro9000 2TB · 2000.0 GiB ·"]
        };
        for fact in facts {
            assert!(
                visited.contains(fact),
                "{width}×{height} System viewport never reached storage fact {fact:?}"
            );
        }
    }
}

/// Unobserved byte figures in the storage row stay honest dashes — never a
/// fabricated total.
#[test]
fn system_storage_row_keeps_unobserved_byte_figures_as_dashes() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::System));
    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        let snapshot = snapshot.as_mut().expect("demo snapshot");
        let mut disk = snapshot
            .disks
            .pop()
            .expect("demo app should carry one disk");
        let mut observations = *disk.scalar_observations();
        observations.capacity_bytes = ScalarObservation::default();
        observations.available_bytes = ScalarObservation::default();
        disk.apply_scalar_observations(observations);
        snapshot.disks.push(disk);
    });

    let mut visited = String::new();
    for offset in 0..80 {
        app.system_scroll = offset;
        visited.push_str(&frame_text(&app, 120, 52));
        visited.push('\n');
    }
    assert!(
        visited.contains("TiPro9000 2TB · — · — · NVMe SSD"),
        "unobserved capacity/available must render honest dashes:\n{visited}"
    );
    assert!(
        !visited.contains("2000.0 GiB"),
        "no fabricated byte total may render:\n{visited}"
    );
}

/// Render one frame in an explicit language. Caller holds `LANG_TEST_GUARD`
/// (`frame_text` would re-lock the non-reentrant mutex).
fn frame_text_in_language(app: &crate::TuiApp, width: u16, height: u16) -> String {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| crate::ui::render(frame, app, crate::TuiTheme::default()))
        .expect("draw");
    terminal.backend().to_string()
}

/// The storage section title resolves through the shared catalog in the
/// active locale: English under En, Chinese under Zh, never hardcoded copy.
#[test]
fn system_storage_section_follows_the_active_locale() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::System));
    app.system_scroll = usize::MAX;
    let guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");

    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let en_frame = frame_text_in_language(&app, 120, 52);
    let en_title = taskmanager_application::i18n::t("system.section.storage");

    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::Zh);
    let zh_frame = frame_text_in_language(&app, 120, 52);
    let zh_title = taskmanager_application::i18n::t("system.section.storage");
    drop(guard);

    assert_ne!(
        en_title, zh_title,
        "system.section.storage must translate to distinct En/Zh copy"
    );
    assert!(
        en_frame.contains(en_title),
        "the En tail frame must paint {en_title:?}:\n{en_frame}"
    );
    assert!(
        zh_frame.contains(zh_title),
        "the Zh tail frame must paint {zh_title:?}:\n{zh_frame}"
    );
    // The disk identity value itself is locale-independent typed data.
    assert!(
        zh_frame.contains("TiPro9000 2TB"),
        "the Zh tail frame keeps the typed disk identity:\n{zh_frame}"
    );
}
