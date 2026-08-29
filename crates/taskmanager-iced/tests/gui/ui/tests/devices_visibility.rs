//! Device-visibility preference + unit-preference formatting tests: the
//! settings-driven ShowDevice filters over the select-a-device rail (family
//! and network-subcategory toggles with the hidden-device fallback) and the
//! bytes/bits/base ladders of the preference-aware rate formatter. Extracted
//! from [`super::devices`] so neither sibling file holds the whole
//! device-section suite.

use super::super::*;
use super::{available_perf_devices, resolved_perf_device};
use crate::test_support::temp_dir;
use taskmanager_application::ConfigStore;

fn visibility_test_app(label: &str) -> (crate::IcedApp, std::path::PathBuf) {
    // An isolated config store (a shared real-path store would be written by
    // every parallel settings test) plus the capture fixture's snapshot so the
    // rail has disks/networks/GPUs to filter.
    let dir = temp_dir(label);
    let path = dir.join("config.json");
    let mut app = crate::IcedApp::with_config_store(None, ConfigStore::new(&path));
    let fixture = crate::IcedApp::demo_for_capture();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(
            fixture.shell.projection().snapshot.clone(),
        )),
    );
    (app, dir)
}

#[test]
fn device_visibility_preferences_filter_the_selector_rail() {
    use crate::app::{DeviceKind, Message, SettingsChange};
    let (mut app, dir) = visibility_test_app("visibility-rail");

    // Everything visible by default: CPU + Memory + the dynamic families.
    let all = available_perf_devices(&app);
    assert!(all.contains(&PerfDevice::Cpu));
    assert!(all.contains(&PerfDevice::Memory));
    assert!(all.contains(&PerfDevice::Disk(0)));
    assert!(all.contains(&PerfDevice::Network(0)));
    assert!(all.contains(&PerfDevice::Gpu(0)));

    // Hiding a family removes its entries from the rail…
    let _ = app.update(Message::SettingsChanged(SettingsChange::ShowDevice(
        DeviceKind::Gpus,
        false,
    )));
    let without_gpu = available_perf_devices(&app);
    assert!(!without_gpu.contains(&PerfDevice::Gpu(0)));
    assert!(without_gpu.contains(&PerfDevice::Disk(0)));

    let _ = app.update(Message::SettingsChanged(SettingsChange::ShowDevice(
        DeviceKind::Cpu,
        false,
    )));
    let without_cpu = available_perf_devices(&app);
    assert!(!without_cpu.contains(&PerfDevice::Cpu));
    assert!(without_cpu.contains(&PerfDevice::Memory));

    // …and a selection whose device was hidden falls back to the first
    // visible device on the next frame.
    let _ = app.update(Message::SelectPerfDevice(PerfDevice::Gpu(0)));
    let _ = app.update(Message::SettingsChanged(SettingsChange::ShowDevice(
        DeviceKind::Gpus,
        false,
    )));
    // CPU was hidden in the step above, so the first still-visible device is
    // Memory: the fallback is the first VISIBLE device, never a blind Cpu.
    assert_eq!(
        resolved_perf_device(&app),
        PerfDevice::Memory,
        "a hidden device falls back to the first visible one"
    );
    let _ = view(&app);
    drop(app);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn network_subcategory_toggles_filter_by_adapter_type() {
    use crate::app::{DeviceKind, Message, SettingsChange};
    use taskmanager_core::core::metrics::NetworkAdapterType;
    let (mut app, dir) = visibility_test_app("visibility-network");

    // Give the fixture a VPN and a loopback interface alongside the demo NIC.
    let mut snapshot = app
        .shell
        .projection()
        .snapshot
        .clone()
        .expect("capture fixture snapshot");
    let mut wired = snapshot.networks[0].clone();
    wired.interface_name = "eth0".into();
    let wired_scalars = *wired.scalar_observations();
    wired.apply_observations(
        NetworkAdapterType::Ethernet,
        wired_scalars,
        taskmanager_core::core::metrics::NetworkWirelessObservations::not_applicable(1),
    );
    snapshot.networks[0] = wired;
    let mut vpn = snapshot.networks[0].clone();
    vpn.interface_name = "tun0".into();
    let vpn_scalars = *vpn.scalar_observations();
    vpn.apply_observations(
        NetworkAdapterType::Vpn,
        vpn_scalars,
        taskmanager_core::core::metrics::NetworkWirelessObservations::not_applicable(1),
    );
    let mut loopback = snapshot.networks[0].clone();
    loopback.interface_name = "lo".into();
    let loopback_scalars = *loopback.scalar_observations();
    loopback.apply_observations(
        NetworkAdapterType::Loopback,
        loopback_scalars,
        taskmanager_core::core::metrics::NetworkWirelessObservations::not_applicable(1),
    );
    snapshot.networks.push(vpn);
    snapshot.networks.push(loopback);
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
    );

    let all = available_perf_devices(&app);
    assert!(all.contains(&PerfDevice::Network(1)), "tun0 visible");
    assert!(all.contains(&PerfDevice::Network(2)), "lo visible");

    // Hiding VPNs removes tun0 but keeps the wired demo NIC and loopback.
    let _ = app.update(Message::SettingsChanged(SettingsChange::ShowDevice(
        DeviceKind::NetworkVpn,
        false,
    )));
    let filtered = available_perf_devices(&app);
    assert!(!filtered.contains(&PerfDevice::Network(1)));
    assert!(filtered.contains(&PerfDevice::Network(0)));
    assert!(
        filtered.contains(&PerfDevice::Network(2)),
        "loopback is Other"
    );

    // Hiding Other removes loopback; the wired NIC remains.
    let _ = app.update(Message::SettingsChanged(SettingsChange::ShowDevice(
        DeviceKind::NetworkOther,
        false,
    )));
    let filtered = available_perf_devices(&app);
    assert!(!filtered.contains(&PerfDevice::Network(2)));
    assert!(filtered.contains(&PerfDevice::Network(0)));

    // Hiding the whole Network family empties the rail of NICs entirely.
    let _ = app.update(Message::SettingsChanged(SettingsChange::ShowDevice(
        DeviceKind::Network,
        false,
    )));
    let filtered = available_perf_devices(&app);
    assert!(
        filtered
            .iter()
            .all(|d| !matches!(d, PerfDevice::Network(_)))
    );
    drop(app);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn rate_text_pref_honors_bytes_bits_and_base_ladders() {
    // The preference-aware formatter keeps the /s suffix and dash honesty.
    assert_eq!(rate_text_pref(None, true, true), "—");
    assert_eq!(rate_text_pref(Some(1_500_000), true, false), "1.5 MB/s");
    assert_eq!(rate_text_pref(Some(1_500_000), false, false), "12.0 Mb/s");
    assert_eq!(rate_text_pref(Some(1_500_000), true, true), "1.4 MiB/s");
    assert_eq!(
        rate_text_pref(Some(84 * 1024 * 1024), false, true),
        "672.0 Mib/s"
    );
}
