//! Performance-device behavior tests: the GPU / Disk / Network / Fan section
//! projections and the select-a-device selector (PerfDevice). These are the
//! pure-function seams the renderer agrees on with the headless suite. (Battery
//! lives in [`super::battery`].)

use super::super::perf_devices::network::{
    network_section_state, network_summary_lines, network_title,
};
use super::super::tables::ListState;
use super::super::*;
use super::{
    CompactDetailViewport, PerfDetail, available_perf_devices, bounded_sidebar_label, chunk_count,
    compact_detail_viewport, perf_detail_kind, performance_sidebar_label,
};
use taskmanager_application::{
    DiskMetrics, GpuScalarObservations, GpuThrottleReason, NetworkMetrics, ScalarObservation,
    SystemSnapshot,
};

#[path = "devices/fan.rs"]
mod fan;

/// The default-unit rate formatter (binary bytes), kept for the headless
/// tests that assert the baseline shape; the view uses [`rate_text_pref`].
pub(crate) fn rate_text(value: Option<u64>) -> String {
    value.map_or_else(|| "—".to_string(), |value| format!("{}/s", bytes(value)))
}

#[test]
fn gpu_summary_projects_real_values_for_a_populated_snapshot() {
    use taskmanager_application::i18n::{Language, set_language};
    use taskmanager_application::{GpuEngine, GpuEngineKind, GpuMetrics};

    // The shared catalog auto-detects the host locale on first use; pin English
    // so the label assertions are deterministic and independent of the host.
    set_language(Language::En);

    let mut gpu = GpuMetrics::new("gpu:pci:0000:03:00.0", "NVIDIA GeForce");
    gpu.device_state = taskmanager_application::DeviceState {
        status: taskmanager_application::DeviceStatus::Healthy,
        ..Default::default()
    };
    gpu.driver = Some("nvidia".into());
    gpu.engines = vec![
        GpuEngine {
            name: "Render/3D".into(),
            kind: GpuEngineKind::Render,
            usage_pct: 77.0,
        },
        GpuEngine {
            name: "Copy".into(),
            kind: GpuEngineKind::Copy,
            usage_pct: 0.0,
        },
    ];
    gpu.apply_scalar_observations(GpuScalarObservations {
        utilization_pct: ScalarObservation::available(42.0, 1),
        dedicated_vram_used_bytes: ScalarObservation::available(4 << 30, 1),
        dedicated_vram_total_bytes: ScalarObservation::available(8 << 30, 1),
        shared_vram_used_bytes: ScalarObservation::available(512 << 20, 1),
        shared_vram_total_bytes: ScalarObservation::available(1 << 30, 1),
        memory_used_bytes: ScalarObservation::available(2 << 30, 1),
        memory_total_bytes: ScalarObservation::available(16 << 30, 1),
        frequency_mhz: ScalarObservation::available(1800, 1),
        max_frequency_mhz: ScalarObservation::available(2100, 1),
        temperature_c: ScalarObservation::available(61.0, 1),
        power_w: ScalarObservation::available(95.0, 1),
        idle_residency_pct: ScalarObservation::available(78.0, 1),
        ..Default::default()
    });
    gpu.apply_throttle_observation(ScalarObservation::available(
        vec![GpuThrottleReason::HardwareThermalLimit],
        1,
    ));

    let rows = gpu_summary_lines(&gpu);
    assert_eq!(
        rows,
        vec![
            ("Status".into(), "Healthy".into()),
            ("Utilization".into(), "42%".into()),
            ("Dedicated VRAM".into(), "4.0 GiB / 8.0 GiB".into()),
            ("Shared VRAM".into(), "512.0 MiB / 1.0 GiB".into()),
            ("VRAM".into(), "2.0 GiB / 16.0 GiB".into()),
            ("Clock".into(), "1800 MHz".into()),
            ("Max clock".into(), "2100 MHz".into()),
            ("Idle residency".into(), "78%".into()),
            ("Temperature".into(), "61 °C".into()),
            ("Power".into(), "95.0 W".into()),
            ("Driver".into(), "nvidia".into()),
            ("Render/3D".into(), "77%".into()),
            // A measured-idle engine stays 0% — never suppressed into "—".
            ("Copy".into(), "0%".into()),
            ("Throttling".into(), "hardware thermal limit".into()),
        ]
    );
    assert_eq!(gpu_title(&gpu), "GPU: NVIDIA GeForce");

    // A measured-idle GPU utilization stays 0% — the honest opposite of "—".
    let idle = GpuMetrics::from_observations(GpuScalarObservations {
        utilization_pct: ScalarObservation::available(0.0, 1),
        ..Default::default()
    });
    let idle_rows = gpu_summary_lines(&idle);
    // Status leads; a measured-idle utilization stays 0% — the honest opposite
    // of "—".
    assert_eq!(idle_rows[0].0, "Status");
    assert_eq!(idle_rows[1], ("Utilization".into(), "0%".into()));

    set_language(Language::En);
}

#[test]
fn gpu_summary_keeps_honest_dashes_for_unavailable_fields() {
    use taskmanager_application::GpuMetrics;
    use taskmanager_application::i18n::{Language, set_language};

    set_language(Language::En);

    // A provider that supplied only identity: every scalar observation is
    // unavailable, so VRAM/clock/power/engines must be omitted rather than
    // fabricated as zero, and utilization/temperature render an honest dash.
    let bare = GpuMetrics::new("gpu:pci:0000:00:02.0", "");
    let rows = gpu_summary_lines(&bare);

    // Status (default Unsupported) leads and utilization remains as the one
    // headline scalar. Unsupported optional facts omit their rows entirely;
    // a large empty panel should not be padded with a column of dashes.
    assert_eq!(
        rows.len(),
        2,
        "unavailable VRAM/clock/power/engines are omitted"
    );
    assert_eq!(rows[0], ("Status".into(), "Unsupported".into()));
    assert_eq!(rows[1], ("Utilization".into(), "—".into()));
    assert!(
        !rows
            .iter()
            .any(|(label, _)| label.contains("VRAM") || label.contains("memory")),
        "no VRAM/memory row may appear without a measured total"
    );
    assert_eq!(gpu_title(&bare), "GPU: gpu:pci:0000:00:02.0");

    // A zero-total VRAM (unified-memory iGPU) is treated as unavailable, not 0/0.
    let zero_total = GpuMetrics::from_observations(GpuScalarObservations {
        dedicated_vram_used_bytes: ScalarObservation::available(0, 1),
        dedicated_vram_total_bytes: ScalarObservation::available(0, 1),
        ..Default::default()
    });
    let zero_rows = gpu_summary_lines(&zero_total);
    assert!(
        !zero_rows
            .iter()
            .any(|(label, _)| label.contains("VRAM") || label.contains("memory")),
        "a zero VRAM total is hidden, never printed as 0 / 0"
    );

    // A GPU with no identity at all still gets the neutral localized heading.
    let unnamed = GpuMetrics::default();
    assert_eq!(gpu_title(&unnamed), "GPU");

    set_language(Language::En);
}

#[test]
fn gpu_summary_promotes_the_pci_marketing_name_without_losing_the_fact_row() {
    use taskmanager_application::GpuMetrics;
    use taskmanager_application::i18n::{Language, set_language};

    set_language(Language::En);
    let mut gpu = GpuMetrics::new("", "Intel Xe Graphics");
    gpu.marketing_name = Some("Arc B390".into());
    assert_eq!(gpu_title(&gpu), "GPU: Arc B390");
    assert!(
        gpu_summary_lines(&gpu)
            .iter()
            .any(|(label, value)| label == "Product name" && value == "Arc B390")
    );
    set_language(Language::En);
}

#[test]
fn gpu_section_state_distinguishes_loading_empty_and_ready() {
    // No snapshot yet → Loading (the collecting state).
    assert_eq!(gpu_section_state(None), ListState::Loading);

    // A snapshot that reports no GPU → Empty (honest absence, not a hidden zero).
    let empty = SystemSnapshot::default();
    assert!(empty.gpu.is_empty());
    assert_eq!(gpu_section_state(Some(&empty)), ListState::Empty);

    // The demo fixture carries a GPU → Ready.
    let shell = taskmanager_shell::demo_app();
    let snapshot = shell
        .projection()
        .snapshot
        .as_ref()
        .expect("demo snapshot fixture must carry a GPU");
    assert_eq!(gpu_section_state(Some(snapshot)), ListState::Ready);
}

#[test]
fn performance_page_renders_the_gpu_section_for_the_demo_snapshot() {
    // The default page is Performance; the demo snapshot carries one GPU (an
    // Intel xe with utilization + clock + temperature). Under the select-a-device
    // model the GPU section is behind its selector tab, so the page must render
    // it when Gpu is selected (and NOT in the default CPU view). The pure seam
    // proves which rows are projected.
    use taskmanager_application::i18n::{Language, set_language};
    set_language(Language::En);
    let mut app = crate::IcedApp::demo();
    let snapshot = app
        .shell
        .projection()
        .snapshot
        .as_ref()
        .expect("demo snapshot carries a GPU");
    assert_eq!(gpu_section_state(Some(snapshot)), ListState::Ready);
    assert_eq!(snapshot.gpu.len(), 1);
    let gpu_rows = gpu_summary_lines(&snapshot.gpu[0]);
    assert_eq!(
        gpu_rows[0].0, "Status",
        "device.status leads the GPU readout"
    );
    assert_eq!(gpu_rows[1].0, "Utilization");
    assert_eq!(gpu_rows[1].1, "18%");
    assert_eq!(gpu_rows[2].1, "900 MHz", "live xe core clock projects");
    assert!(
        gpu_rows.iter().any(|(label, _)| label == "Temperature"),
        "the demo GPU temperature row is present"
    );
    assert!(
        !gpu_rows
            .iter()
            .any(|(label, _)| label.contains("VRAM") || label.contains("memory")),
        "the unified-memory demo GPU honestly omits VRAM"
    );

    // The default view is CPU; the GPU panel is NOT the default detail.
    assert_eq!(app.perf_device(), PerfDevice::Cpu);
    assert_eq!(perf_detail_kind(PerfDevice::Cpu), PerfDetail::CpuOrMemory);
    let _ = view(&app);

    // Selecting Gpu routes the selector to the GPU detail panel and renders it.
    let _ = app.update(Message::SelectPerfDevice(PerfDevice::Gpu(0)));
    assert_eq!(app.perf_device(), PerfDevice::Gpu(0));
    assert_eq!(perf_detail_kind(PerfDevice::Gpu(0)), PerfDetail::Gpu);
    let _view_gpu = view(&app);
}

#[test]
fn perf_device_default_selection_is_cpu() {
    // A fresh frontend defaults to the CPU overview (MC's default view), and
    // the CPU/Memory overview is the detail panel the default selection maps to.
    let app = crate::IcedApp::default();
    assert_eq!(app.perf_device(), PerfDevice::Cpu);
    assert_eq!(
        PerfDevice::default(),
        PerfDevice::Cpu,
        "the default selector must be CPU"
    );
    let demo = crate::IcedApp::demo();
    assert_eq!(demo.perf_device(), PerfDevice::Cpu);
}

#[test]
fn perf_device_selector_tabs_cover_every_variant_with_a_localized_label() {
    use taskmanager_application::i18n::{Language, set_language};
    set_language(Language::En);

    // The selector iterates PerfDevice::ALL (not a duplicated list), so every
    // variant gets a tab. Each label reuses an existing catalog key and is
    // non-empty; each tab's focus-operation id is unique and tab-bound.
    assert_eq!(
        PerfDevice::ALL.len(),
        7,
        "exactly seven Performance resources are selectable"
    );

    let labels: Vec<&'static str> = PerfDevice::ALL.into_iter().map(perf_device_label).collect();
    assert!(labels.iter().all(|label| !label.is_empty()));
    // The two singleton resources still carry distinct labels.
    assert_eq!(perf_device_label(PerfDevice::Cpu), "CPU");
    assert_eq!(perf_device_label(PerfDevice::Memory), "Memory");
    assert_eq!(perf_device_label(PerfDevice::Gpu(0)), "GPU");
    assert_eq!(perf_device_label(PerfDevice::Disk(0)), "Disk");
    assert_eq!(perf_device_label(PerfDevice::Network(0)), "Network");
    assert_eq!(perf_device_label(PerfDevice::Battery(0)), "Battery");
    assert_eq!(perf_device_label(PerfDevice::Fan(0)), "Fan");

    let ids: Vec<String> = PerfDevice::ALL
        .into_iter()
        .map(|device| crate::focus::focus_id(crate::app::FocusTarget::PerfDeviceTab(device)))
        .collect();
    let unique: std::collections::HashSet<&String> = ids.iter().collect();
    assert_eq!(ids.len(), unique.len(), "perf-tab focus ids must be unique");
    assert_eq!(
        crate::focus::focus_id(crate::app::FocusTarget::PerfDeviceTab(PerfDevice::Gpu(0))),
        "iced-perf-tab-gpu-0"
    );
    assert_eq!(
        crate::focus::focus_id(crate::app::FocusTarget::PerfDeviceTab(PerfDevice::Battery(
            0
        ))),
        "iced-perf-tab-battery-0"
    );
    assert_eq!(
        crate::focus::focus_id(crate::app::FocusTarget::PerfDeviceTab(PerfDevice::Fan(0))),
        "iced-perf-tab-fan-0"
    );

    set_language(Language::En);
}

#[test]
fn performance_rail_keeps_dynamic_device_indices_like_gpui() {
    use taskmanager_application::i18n::{Language, set_language};
    set_language(Language::En);
    let mut app = crate::IcedApp::demo_for_capture();
    let mut snapshot = app
        .shell
        .projection()
        .snapshot
        .clone()
        .expect("capture fixture snapshot");

    let mut second_disk = snapshot.disks[0].clone();
    second_disk.name = "nvme1n1".into();
    snapshot.disks.push(second_disk);
    let mut second_network = snapshot.networks[0].clone();
    second_network.interface_name = "eth1".into();
    snapshot.networks.push(second_network);
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
    );

    let devices = available_perf_devices(&app);
    assert!(devices.contains(&PerfDevice::Disk(0)));
    assert!(devices.contains(&PerfDevice::Disk(1)));
    assert!(devices.contains(&PerfDevice::Network(0)));
    assert!(devices.contains(&PerfDevice::Network(1)));
    assert_eq!(
        performance_sidebar_label(&app, PerfDevice::Disk(1)),
        "Disk  nvme1n1"
    );
    assert_eq!(
        performance_sidebar_label(&app, PerfDevice::Network(1)),
        "Network  eth1"
    );
}

#[test]
fn performance_layout_switches_at_the_gpui_viewport_contract() {
    let mut app = crate::IcedApp::demo_for_capture();
    assert!(!app.compact_layout());

    let _ = app.update(Message::WindowResized(iced::Size::new(720.0, 480.0)));
    assert!(app.compact_layout());
    let _ = view(&app);

    let _ = app.update(Message::WindowResized(iced::Size::new(1180.0, 780.0)));
    assert!(!app.compact_layout());
    let _ = view(&app);
}

#[test]
fn compact_performance_view_composes_at_a_narrow_docked_width() {
    let mut app = crate::IcedApp::demo_for_capture();
    let _ = app.update(Message::WindowResized(iced::Size::new(520.0, 460.0)));
    assert!(app.compact_layout());
    let _ = view(&app);
}

#[test]
fn compact_cpu_and_gpu_own_elastic_viewports_while_variable_device_pages_still_scroll() {
    assert_eq!(
        compact_detail_viewport(PerfDevice::Cpu),
        CompactDetailViewport::Elastic
    );
    assert_eq!(
        compact_detail_viewport(PerfDevice::Gpu(0)),
        CompactDetailViewport::Elastic
    );
    assert_eq!(
        compact_detail_viewport(PerfDevice::Memory),
        CompactDetailViewport::Scrollable
    );
    assert_eq!(
        compact_detail_viewport(PerfDevice::Disk(0)),
        CompactDetailViewport::Scrollable
    );
}

#[test]
fn compact_device_labels_are_bounded_without_losing_the_family_name() {
    assert_eq!(bounded_sidebar_label("CPU 39%", 18), "CPU 39%");
    assert_eq!(
        bounded_sidebar_label("GPU Intel Core Ultra Graphics", 18),
        "GPU Intel Core Ul…"
    );
    assert_eq!(
        bounded_sidebar_label("网络设备很长的型号", 6),
        "网络设备很…"
    );
}

#[test]
fn compact_control_rows_have_a_bounded_row_count() {
    assert_eq!(chunk_count(0, 3), 0);
    assert_eq!(chunk_count(5, 3), 2);
    assert_eq!(chunk_count(7, 4), 2);
    assert_eq!(chunk_count(1, 0), 1);
}

#[test]
fn compact_toolbar_stays_single_row_when_the_route_strip_has_room() {
    assert_eq!(compact_toolbar_columns(720.0), 5);
    assert_eq!(compact_toolbar_columns(520.0), 3);
}

#[test]
fn compact_device_selector_uses_a_bounded_horizontal_window() {
    let first =
        VirtualWindow::for_columns(100, 0.0, 720.0, perf_rail::COMPACT_DEVICE_ITEM_WIDTH, 0.0);
    let middle = VirtualWindow::for_columns(
        100,
        1_560.0,
        520.0,
        perf_rail::COMPACT_DEVICE_ITEM_WIDTH,
        0.0,
    );

    assert_eq!(first.start, 0);
    assert!(first.end < 100);
    assert!(middle.start > first.start);
    assert!(middle.end > middle.start);
    assert!(middle.top > first.top);
}

#[test]
fn perf_detail_kind_maps_each_selector_to_its_panel() {
    // CPU and Memory share one singleton dispatch entry; each renderer then
    // projects its own fixed content.
    assert_eq!(perf_detail_kind(PerfDevice::Cpu), PerfDetail::CpuOrMemory);
    assert_eq!(
        perf_detail_kind(PerfDevice::Memory),
        PerfDetail::CpuOrMemory
    );
    assert_eq!(perf_detail_kind(PerfDevice::Disk(0)), PerfDetail::Disk);
    assert_eq!(
        perf_detail_kind(PerfDevice::Network(0)),
        PerfDetail::Network
    );
    assert_eq!(perf_detail_kind(PerfDevice::Gpu(0)), PerfDetail::Gpu);
    assert_eq!(
        perf_detail_kind(PerfDevice::Battery(0)),
        PerfDetail::Battery
    );
}

#[test]
fn select_perf_device_message_updates_the_selector_and_routes_focus() {
    // Driving the selector message updates the frontend-local field; the focus
    // target follows the selected tab so keyboard users land on it. The shell
    // state is untouched (the selector never crosses into shared state).
    let mut app = crate::IcedApp::demo();
    assert_eq!(app.perf_device(), PerfDevice::Cpu);

    let _ = app.update(Message::SelectPerfDevice(PerfDevice::Network(0)));
    assert_eq!(app.perf_device(), PerfDevice::Network(0));

    // The full Performance page constructs with the new selection (the network
    // panel renders its honest state for the demo snapshot).
    let _ = view(&app);

    // Selecting Memory stays on the combined overview panel (not a device
    // section), proving Cpu/Memory reuse the gauges+chart rather than a split.
    let _ = app.update(Message::SelectPerfDevice(PerfDevice::Memory));
    assert_eq!(app.perf_device(), PerfDevice::Memory);
    assert_eq!(perf_detail_kind(app.perf_device()), PerfDetail::CpuOrMemory);
    let _view_mem = view(&app);
}

#[test]
fn perf_device_selector_renders_for_every_selection_without_panicking() {
    // The whole page (selector row + the selected detail) must compose for every
    // selectable device, including the no-data honest states a fresh frontend
    // reaches before the first snapshot arrives.
    let mut app = crate::IcedApp::default();
    assert!(app.shell.projection().snapshot.is_none());
    for device in PerfDevice::ALL {
        let _ = app.update(Message::SelectPerfDevice(device));
        assert_eq!(app.perf_device(), device);
        let _view = view(&app);
    }
}

#[test]
fn rate_text_formats_bytes_per_second_and_keeps_measured_zero_honest() {
    // A measured idle rate stays a real value — never collapsed to "—".
    assert_eq!(rate_text(Some(0)), "0 B/s");
    assert_eq!(rate_text(Some(1536)), "1.5 KiB/s");
    assert_eq!(rate_text(Some(84 * 1024 * 1024)), "84.0 MiB/s");
    // An unobserved rate renders an honest dash, not a fabricated 0.
    assert_eq!(rate_text(None), "—");
}

#[test]
fn disk_summary_projects_real_rates_active_time_smart_and_partition_space() {
    use taskmanager_application::i18n::{Language, set_language};

    // The shared catalog auto-detects the host locale on first use; pin English
    // so the label assertions are deterministic and independent of the host.
    set_language(Language::En);

    const GIB: u64 = 1024 * 1024 * 1024;
    const MIB: u64 = 1024 * 1024;

    let mut disk = taskmanager_test_support::DiskMetricsFixtureBuilder::new()
        .device_id("disk:test:nvme1".into())
        .name("nvme1n1".into())
        .disk_type("SATA SSD".into())
        .mount_point("/".into())
        .device_state(taskmanager_application::DeviceState {
            status: taskmanager_application::DeviceStatus::Healthy,
            ..Default::default()
        })
        .current_capacity_bytes(500 * GIB)
        .current_available_bytes(250 * GIB)
        .current_read_bytes_per_sec(100 * MIB)
        .current_write_bytes_per_sec(40 * MIB)
        .current_active_time_pct(5.4)
        .smart_temperature_c(Some(33.0))
        .smart_percent_used(Some(2.0))
        .smart_power_on_hours(Some(7200))
        .build();
    disk.partitions = vec![
        taskmanager_test_support::DiskPartitionFixtureBuilder::new()
            .mount_point("/home".into())
            .name("nvme1n1p2".into())
            .scalar_observations(taskmanager_application::DiskPartitionScalarObservations {
                capacity_bytes: ScalarObservation::available(100 * GIB, 1),
                used_bytes: ScalarObservation::available(70 * GIB, 1),
                free_bytes: ScalarObservation::available(30 * GIB, 1),
            })
            .build(),
    ];

    let rows = disk_summary_lines(&disk, true, true);
    assert_eq!(
        rows,
        vec![
            ("Status".into(), "Healthy".into()),
            ("Read".into(), "100.0 MiB/s".into()),
            ("Write".into(), "40.0 MiB/s".into()),
            ("Active time".into(), "5%".into()),
            ("Capacity".into(), "500.0 GiB".into()),
            ("Free".into(), "250.0 GiB".into()),
            ("Type".into(), "SATA SSD".into()),
            ("Temperature".into(), "33 °C".into()),
            ("Endurance used".into(), "2%".into()),
            ("Power-on".into(), "7200 h (300 d)".into()),
            ("Partitions · /home".into(), "70.0 GiB / 100.0 GiB".into()),
        ]
    );
    assert_eq!(disk_title(&disk), "Disk: nvme1n1");

    set_language(Language::En);
}

#[test]
fn disk_summary_keeps_honest_dashes_and_omits_unavailable_scalars() {
    use taskmanager_application::i18n::{Language, set_language};
    set_language(Language::En);

    // A bare disk: every typed observation is unavailable, so read/write/active
    // render honest dashes and response/IOPS/capacity/free/SMART/partitions are
    // omitted rather than fabricated as zero. The SMART section is hidden too:
    // a provider that could not supply readings has nothing to show.
    let bare = DiskMetrics::default();
    let rows = disk_summary_lines(&bare, true, true);
    assert_eq!(
        rows,
        vec![
            // The default DeviceState is Unsupported (DeviceStatus::default),
            // so a bare disk surfaces its status honestly as the first row.
            ("Status".into(), "Unsupported".into()),
            ("Read".into(), "—".into()),
            ("Write".into(), "—".into()),
            ("Active time".into(), "—".into()),
        ]
    );
    assert!(
        !rows
            .iter()
            .any(|(label, _)| label == "Capacity" || label == "Free"),
        "capacity/free stay hidden when no total is observed"
    );
    assert!(
        !rows.iter().any(|(label, _)| label == "Temperature"),
        "SMART temperature is omitted when unobserved"
    );
    assert_eq!(disk_title(&bare), "Disk", "no identity → neutral heading");

    set_language(Language::En);
}

#[test]
fn disk_summary_surfaces_critical_warning_prefix_and_removable_flag() {
    use taskmanager_application::i18n::{Language, set_language};
    set_language(Language::En);

    // A disk whose hwmon layer raised the critical-warning bit prefixes the
    // temperature label with ⚠ (the most actionable SMART fact), mirroring GPUI.
    let mut disk = taskmanager_test_support::DiskMetricsFixtureBuilder::new()
        .smart_temperature_c(Some(81.0))
        .smart_critical_warning(Some(true))
        .smart_temp_critical_c(Some(70.0))
        .build();
    let rows = disk_summary_lines(&disk, true, true);
    let temp = rows
        .iter()
        .find(|(label, _)| label.starts_with("Temperature"))
        .expect("temperature row must render");
    assert!(
        temp.0.contains('\u{26a0}'),
        "critical-warning must prefix the label with ⚠: {}",
        temp.0
    );
    assert_eq!(temp.1, "81 / 70 °C");

    // Without the warning bit the label is the plain localized temperature.
    disk.smart_critical_warning = None;
    let rows = disk_summary_lines(&disk, true, true);
    let temp = rows
        .iter()
        .find(|(label, _)| label.starts_with("Temperature"))
        .expect("temperature row must render");
    assert!(!temp.0.contains('\u{26a0}'));

    // A removable device (USB / optical) gains a Removable = Yes row.
    disk.apply_attachment_capabilities(Some(true), disk.hotplug_capable());
    let rows = disk_summary_lines(&disk, true, true);
    assert!(
        rows.iter()
            .any(|(label, value)| label == "Removable" && value == "Yes"),
        "a removable device must surface a Removable = Yes row"
    );

    set_language(Language::En);
}

#[test]
fn disk_section_state_distinguishes_loading_empty_and_ready() {
    // No snapshot yet → Loading (the collecting state).
    assert_eq!(disk_section_state(None), ListState::Loading);

    // A snapshot that reports no disk → Empty (honest absence, not a hidden zero).
    let empty = SystemSnapshot::default();
    assert!(empty.disks.is_empty());
    assert_eq!(disk_section_state(Some(&empty)), ListState::Empty);

    // The demo fixture carries one disk → Ready.
    let shell = taskmanager_shell::demo_app();
    let snapshot = shell
        .projection()
        .snapshot
        .as_ref()
        .expect("demo snapshot fixture must carry a disk");
    assert_eq!(disk_section_state(Some(snapshot)), ListState::Ready);
    assert_eq!(snapshot.disks.len(), 1);
}

#[test]
fn network_summary_projects_wireless_ssid_signal_and_utilization() {
    use taskmanager_application::i18n::{Language, set_language};
    set_language(Language::En);

    const MIB: u64 = 1024 * 1024;
    let nic = taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
        .device_id("network:test:wlp3s0".into())
        .interface_name("wlp3s0".into())
        .current_rx_bytes_per_sec(5 * MIB)
        .current_tx_bytes_per_sec(MIB)
        .current_utilization_pct(22.0)
        .link_speed_observation(match Some(866) {
            Some(value) => taskmanager_application::ScalarObservation::available(value, 1),
            None => taskmanager_application::ScalarObservation::default(),
        })
        .ssid_observation(match Some("TaskForest-5G".into()) {
            Some(value) => taskmanager_application::OptionalObservation::present(value, 1),
            None => taskmanager_application::OptionalObservation::default(),
        })
        .adapter_type(taskmanager_application::NetworkAdapterType::WiFi)
        .signal_observation(match Some(-47) {
            Some(value) => taskmanager_application::OptionalObservation::present(value, 1),
            None => taskmanager_application::OptionalObservation::default(),
        })
        .ipv4_addr(Some("192.168.1.10".into()))
        .ipv6_addr(Some("fe80::2".into()))
        .mac_addr(Some("aa:bb:cc:dd:ee:ff".into()))
        .current_total_rx_bytes(2 * 1024 * 1024 * 1024)
        .current_total_tx_bytes(512 * 1024 * 1024)
        .driver(Some("iwlwifi".into()))
        .adapter(Some("Intel AX201".into()))
        .build();

    let rows = network_summary_lines(&nic, true, true);
    assert_eq!(
        rows,
        vec![
            ("Status".into(), "Unsupported".into()),
            ("Receive".into(), "5.0 MiB/s".into()),
            ("Send".into(), "1.0 MiB/s".into()),
            ("Link".into(), "866 Mbps".into()),
            ("Type".into(), "Wireless".into()),
            ("IPv4".into(), "192.168.1.10".into()),
            ("IPv6".into(), "fe80::2".into()),
            ("MAC".into(), "aa:bb:cc:dd:ee:ff".into()),
            ("Connection".into(), "Connected".into()),
            ("Total received".into(), "2.0 GiB".into()),
            ("Total sent".into(), "512.0 MiB".into()),
            ("Driver".into(), "iwlwifi".into()),
            ("Adapter".into(), "Intel AX201".into()),
            ("Utilization".into(), "22%".into()),
            ("SSID".into(), "TaskForest-5G".into()),
            // -47 dBm → (43/60)*100 ≈ 72% quality.
            ("Signal".into(), "-47 dBm (72%)".into()),
        ]
    );
    assert_eq!(network_title(&nic), "Wireless: wlp3s0");

    set_language(Language::En);
}

#[test]
fn network_summary_omits_utilization_without_a_link_and_keeps_dashes_honest() {
    use taskmanager_application::i18n::{Language, set_language};
    set_language(Language::En);

    // A bare adapter: every typed observation is unavailable, and without a
    // known link speed utilization is omitted rather than read as 0%.
    let bare = NetworkMetrics::default();
    let rows = network_summary_lines(&bare, true, true);
    assert_eq!(
        rows,
        vec![
            ("Status".into(), "Unsupported".into()),
            ("Receive".into(), "—".into()),
            ("Send".into(), "—".into()),
            ("Link".into(), "—".into()),
            ("Type".into(), "Other".into()),
            ("IPv4".into(), "—".into()),
            ("IPv6".into(), "—".into()),
            ("MAC".into(), "—".into()),
            // No carrier and no assigned address → Disconnected (never a
            // fabricated "Connected").
            ("Connection".into(), "Disconnected".into()),
        ]
    );
    assert!(
        !rows.iter().any(|(label, _)| label == "Utilization"),
        "utilization is hidden when no link speed is known"
    );
    assert!(
        !rows
            .iter()
            .any(|(label, _)| label == "SSID" || label == "Signal"),
        "wireless rows stay hidden for a non-wireless adapter"
    );
    assert_eq!(
        network_title(&bare),
        "Other",
        "no identity → the typed category alone"
    );

    set_language(Language::En);
}

#[test]
fn network_section_state_distinguishes_loading_empty_and_ready() {
    assert_eq!(network_section_state(None), ListState::Loading);

    let empty = SystemSnapshot::default();
    assert!(empty.networks.is_empty());
    assert_eq!(network_section_state(Some(&empty)), ListState::Empty);

    let shell = taskmanager_shell::demo_app();
    let snapshot = shell
        .projection()
        .snapshot
        .as_ref()
        .expect("demo snapshot fixture must carry a network");
    assert_eq!(network_section_state(Some(snapshot)), ListState::Ready);
    assert_eq!(snapshot.networks.len(), 1);
}

#[test]
fn performance_page_renders_disk_and_network_sections_for_the_demo_snapshot() {
    use taskmanager_application::i18n::{Language, set_language};
    set_language(Language::En);

    // The demo snapshot carries one disk (nvme0n1, NVMe SSD, 84/31 MiB/s,
    // 12.7% active) and one network adapter (wlan0, 12/2 MiB/s, wired). The
    // pure seams prove the projected rows; under the select-a-device model each
    // panel renders when its tab is selected.
    let mut app = crate::IcedApp::demo();
    let snapshot = app
        .shell
        .projection()
        .snapshot
        .as_ref()
        .expect("demo snapshot fixture");
    assert_eq!(disk_section_state(Some(snapshot)), ListState::Ready);
    assert_eq!(network_section_state(Some(snapshot)), ListState::Ready);

    let disk_rows = disk_summary_lines(&snapshot.disks[0], true, true);
    assert_eq!(disk_title(&snapshot.disks[0]), "Disk: nvme0n1");
    assert_eq!(
        disk_rows
            .iter()
            .find(|(label, _)| label == "Read")
            .map(|(_, value)| value.as_str()),
        Some("84.0 MiB/s")
    );
    assert_eq!(
        disk_rows
            .iter()
            .find(|(label, _)| label == "Write")
            .map(|(_, value)| value.as_str()),
        Some("31.0 MiB/s")
    );
    assert_eq!(
        disk_rows
            .iter()
            .find(|(label, _)| label == "Active time")
            .map(|(_, value)| value.as_str()),
        Some("13%")
    );
    assert_eq!(
        disk_rows
            .iter()
            .find(|(label, _)| label == "Type")
            .map(|(_, value)| value.as_str()),
        Some("NVMe SSD")
    );
    // The demo disk has no SMART provider and no partitions: those rows stay
    // honestly absent rather than printing fabricated zeros.
    assert!(
        !disk_rows
            .iter()
            .any(|(label, _)| label == "Temperature" || label.starts_with("Partitions")),
        "demo disk SMART/partition rows stay hidden when unobserved"
    );

    let net_rows = network_summary_lines(&snapshot.networks[0], true, true);
    assert_eq!(network_title(&snapshot.networks[0]), "Wireless: wlan0");
    assert_eq!(
        net_rows
            .iter()
            .find(|(label, _)| label == "Receive")
            .map(|(_, value)| value.as_str()),
        Some("12.0 MiB/s")
    );
    assert_eq!(
        net_rows
            .iter()
            .find(|(label, _)| label == "Send")
            .map(|(_, value)| value.as_str()),
        Some("2.0 MiB/s")
    );
    // The demo wlan0 fixture is not flagged wireless and has no link speed, so
    // SSID/signal/utilization are honestly omitted and link renders a dash.
    assert_eq!(
        net_rows
            .iter()
            .find(|(label, _)| label == "Link")
            .map(|(_, value)| value.as_str()),
        Some("—")
    );
    assert_eq!(
        net_rows
            .iter()
            .find(|(label, _)| label == "Type")
            .map(|(_, value)| value.as_str()),
        Some("Wireless")
    );
    // The demo NIC is a typed WiFi adapter carrying an SSID, so the SSID row
    // shows it; signal is unobserved and utilization needs a link speed, so
    // those stay honestly hidden.
    assert_eq!(
        net_rows
            .iter()
            .find(|(label, _)| label == "SSID")
            .map(|(_, value)| value.as_str()),
        Some("TaskForest Lab")
    );
    assert!(
        !net_rows
            .iter()
            .any(|(label, _)| label == "Signal" || label == "Utilization"),
        "demo NIC signal/utilization rows stay hidden without observations"
    );

    // The select-a-device model: each panel renders only when its tab is the
    // selected detail. The default (CPU) view does not show disk/network.
    assert_eq!(perf_detail_kind(PerfDevice::Disk(0)), PerfDetail::Disk);
    assert_eq!(
        perf_detail_kind(PerfDevice::Network(0)),
        PerfDetail::Network
    );
    let _ = app.update(Message::SelectPerfDevice(PerfDevice::Disk(0)));
    assert_eq!(app.perf_device(), PerfDevice::Disk(0));
    let _ = view(&app);
    let _ = app.update(Message::SelectPerfDevice(PerfDevice::Network(0)));
    assert_eq!(app.perf_device(), PerfDevice::Network(0));
    let _ = view(&app);
    // The default CPU view (gauges + chart + summary) still constructs.
    let _ = app.update(Message::SelectPerfDevice(PerfDevice::Cpu));
    assert_eq!(app.perf_device(), PerfDevice::Cpu);
    let _view_cpu = view(&app);

    set_language(Language::En);
}
