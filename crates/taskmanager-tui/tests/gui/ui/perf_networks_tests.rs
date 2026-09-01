use super::*;
use taskmanager_core::core::device_state::{DeviceState, DeviceStatus};
use taskmanager_shell::presentation::wifi_signal_quality_percent;

/// Flatten a ratatui `Line` back to its raw text so a test can assert on
/// the rendered string.
fn line_text(line: &ratatui::text::Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

/// Pin English for the duration of `body` so assertions on localized
/// labels (resolved through the process-global `t()`) cannot leak the host
/// locale or a concurrent language-flip test.
fn with_english(body: impl FnOnce()) {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    body();
}

fn wireless_network(signal_dbm: Option<i32>) -> NetworkMetrics {
    taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
        .device_id("network:test:wlan0".into())
        .interface_name("wlan0".into())
        .adapter_type(if true {
            taskmanager_core::core::metrics::NetworkAdapterType::WiFi
        } else {
            taskmanager_core::core::metrics::NetworkAdapterType::Unknown
        })
        .ssid_observation(match Some("Lab".into()) {
            Some(value) => taskmanager_core::core::metrics::OptionalObservation::present(value, 1),
            None => taskmanager_core::core::metrics::OptionalObservation::default(),
        })
        .signal_observation(match signal_dbm {
            Some(value) => taskmanager_core::core::metrics::OptionalObservation::present(value, 1),
            None => taskmanager_core::core::metrics::OptionalObservation::default(),
        })
        .build()
}

/// The RSSI→quality mapping the wireless readout consumes is the shell's
/// single fold (ADR-020), shared with the desktop frontends: -90 dBm floors
/// at 0%, -30 dBm ceilings at 100%, and the mid-range scales linearly.
#[test]
fn the_shared_signal_quality_fold_maps_the_standard_rssi_range() {
    let cases = [
        (-90, 0.0),
        (-75, 25.0),
        (-60, 50.0),
        (-52, 63.33),
        (-45, 75.0),
        (-30, 100.0),
    ];
    for (dbm, expected) in cases {
        let quality = wifi_signal_quality_percent(dbm);
        assert!(
            (quality - expected).abs() < 0.01,
            "signal {dbm} dBm should map to {expected}%, got {quality}%"
        );
    }
    // Out-of-range readings clamp to the honest bounds.
    assert_eq!(wifi_signal_quality_percent(-100), 0.0);
    assert_eq!(wifi_signal_quality_percent(-20), 100.0);
}

/// A wireless adapter with an observed signal renders the derived quality
/// percentage beside the dBm readout (GPUI parity); a wireless adapter
/// without a signal renders an honest dash and never invents a percentage.
#[test]
fn wireless_signal_renders_quality_only_from_an_observed_dbm() {
    with_english(|| {
        let shell = taskmanager_shell::ShellApp::new();
        let with_signal = network_lines(
            &[&wireless_network(Some(-52))],
            &shell,
            TuiTheme::default(),
            true,
            true,
            60,
        );
        let signal_line = with_signal
            .iter()
            .map(line_text)
            .find(|text| text.contains("Signal"))
            .expect("wireless signal line");
        assert!(
            signal_line.contains("-52 dBm (63%)"),
            "observed signal must carry the derived quality:\n{signal_line}"
        );
        assert!(signal_line.contains("Lab"), "SSID missing:\n{signal_line}");

        let no_signal = network_lines(
            &[&wireless_network(None)],
            &shell,
            TuiTheme::default(),
            true,
            true,
            60,
        );
        let signal_line = no_signal
            .iter()
            .map(line_text)
            .find(|text| text.contains("Signal"))
            .expect("wireless signal line");
        assert!(
            signal_line.contains("—"),
            "missing signal must render an honest dash:\n{signal_line}"
        );
        assert!(
            !signal_line.contains('%'),
            "a missing signal must not fabricate a quality percentage:\n{signal_line}"
        );
    });
}

// test-intent: behavior
/// The NIC block's two direction rows project this adapter's OWN split
/// windows on one shared scale: the rising receive spans the full ramp while
/// the constant transmit — pinned at the pair's maximum — rides the top
/// block, and the summed Throughput summary stays beneath the pair.
#[test]
fn network_direction_rows_share_one_scale_and_keep_the_summed_summary() {
    taskmanager_test_support::pin_english();
    let mut shell = taskmanager_shell::ShellApp::new();
    // rx varies 1→3 MiB/s while tx stays pinned at 3 MiB/s.
    for (timestamp_ms, rx, tx) in [
        (1_u64, 1_048_576_u64, 3_145_728_u64),
        (2, 2_097_152, 3_145_728),
        (3, 3_145_728, 3_145_728),
    ] {
        taskmanager_shell::fixture::record_demo_history_frame(
            &mut shell,
            &taskmanager_core::core::metrics::SystemSnapshot {
                timestamp_ms,
                networks: vec![
                    taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
                        .device_id("network:test:eth0".into())
                        .interface_name("eth0".into())
                        .current_rx_bytes_per_sec(rx)
                        .current_tx_bytes_per_sec(tx)
                        .build(),
                ],
                ..taskmanager_core::core::metrics::SystemSnapshot::default()
            },
            None,
            None,
        );
    }

    let nic = taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
        .device_id("network:test:eth0".into())
        .interface_name("eth0".into())
        // The demo seeding resets rings for generation 1; the rendered row
        // carries that bound generation like a real platform row would.
        .device_generation(taskmanager_core::core::identity::DeviceGeneration::new(1))
        .build();
    let lines = network_lines(&[&nic], &shell, TuiTheme::default(), true, true, 60);
    // Index 0 is the header and index 1 the device-status row, so the
    // direction pair starts at index 2.
    let receive_row = line_text(&lines[2]);
    let send_row = line_text(&lines[3]);
    assert!(
        receive_row.contains("Receive") && receive_row.contains("▁▅█"),
        "receive row must label its direction and span the shared ramp: {receive_row:?}"
    );
    assert!(
        send_row.contains("Send") && send_row.contains("███") && !send_row.contains('▅'),
        "send row must ride the shared maximum, not a per-row mid-ramp: {send_row:?}"
    );
    assert!(
        line_text(&lines[4]).contains("Throughput"),
        "the summed total summary stays under the direction pair"
    );
}

// test-intent: behavior
/// The device-status row (GPUI network_stats first stat, §2.4 B-1) carries
/// the typed DeviceStatus vocabulary: a stale adapter renders its degraded
/// health verdict while the carrier-based Connection verdict below
/// independently stays Connected — device health and link state are distinct
/// facts, not an up/down fold.
#[test]
fn network_status_row_expresses_degraded_health_beyond_the_link_verdict() {
    taskmanager_test_support::pin_english();
    let mut stale = wireless_network(None);
    stale.device_state = DeviceState {
        status: DeviceStatus::Stale,
        last_success_ms: Some(1),
    };
    // An assigned address does not replace the missing carrier observation;
    // device health and link state remain independent typed facts.
    stale.ipv4_addr = Some("192.168.1.10".into());
    let texts: Vec<String> = network_lines(
        &[&stale],
        &taskmanager_shell::ShellApp::new(),
        TuiTheme::default(),
        true,
        true,
        60,
    )
    .iter()
    .map(line_text)
    .collect();
    let status_row = texts
        .iter()
        .find(|text| text.trim_start().starts_with("Status "))
        .expect("every NIC renders its device-status row");
    assert!(
        status_row.contains("Stale data"),
        "a stale NIC must express degraded health: {status_row:?}"
    );
    let connection_row = texts
        .iter()
        .find(|text| text.contains("Connection"))
        .expect("the link/connection row stays present");
    assert!(
        connection_row.contains("—"),
        "an unknown carrier must stay an honest gap: {connection_row:?}"
    );
    assert!(
        !connection_row.contains("Stale"),
        "the connection verdict must not fold the health vocabulary in"
    );

    // The healthy fixture value renders the healthy copy on the same row.
    let mut healthy = wireless_network(None);
    healthy.device_state = DeviceState::healthy(1);
    let texts: Vec<String> = network_lines(
        &[&healthy],
        &taskmanager_shell::ShellApp::new(),
        TuiTheme::default(),
        true,
        true,
        60,
    )
    .iter()
    .map(line_text)
    .collect();
    let status_row = texts
        .iter()
        .find(|text| text.trim_start().starts_with("Status "))
        .expect("every NIC renders its device-status row");
    assert!(
        status_row.contains("Healthy"),
        "a healthy NIC must read Healthy: {status_row:?}"
    );
}

// test-intent: behavior
/// The status row resolves through the shared catalog in the active locale:
/// the same typed state paints the English copy under En and the Chinese
/// copy under Zh, each read live through `t()`.
#[test]
fn network_status_row_renders_the_active_locale_copy() {
    let mut stale = wireless_network(None);
    stale.device_state = DeviceState {
        status: DeviceStatus::Stale,
        last_success_ms: Some(1),
    };
    let keys = ["device.status", "device.stale"];
    let guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let en_texts: Vec<String> = network_lines(
        &[&stale],
        &taskmanager_shell::ShellApp::new(),
        TuiTheme::default(),
        true,
        true,
        60,
    )
    .iter()
    .map(line_text)
    .collect();
    let en_labels: Vec<&'static str> = keys.iter().map(|key| t(key)).collect();

    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::Zh);
    let zh_texts: Vec<String> = network_lines(
        &[&stale],
        &taskmanager_shell::ShellApp::new(),
        TuiTheme::default(),
        true,
        true,
        60,
    )
    .iter()
    .map(line_text)
    .collect();
    let zh_labels: Vec<&'static str> = keys.iter().map(|key| t(key)).collect();
    drop(guard);

    for (key, en, zh) in keys
        .iter()
        .zip(en_labels.iter())
        .zip(zh_labels.iter())
        .map(|((key, en), zh)| (*key, *en, *zh))
    {
        assert_ne!(en, zh, "{key} must translate to distinct En/Zh copy");
        assert!(
            en_texts.iter().any(|text| text.contains(en)),
            "En rows must paint {en:?}:\n{en_texts:?}"
        );
        assert!(
            zh_texts.iter().any(|text| text.contains(zh)),
            "Zh rows must paint {zh:?}:\n{zh_texts:?}"
        );
    }
}
