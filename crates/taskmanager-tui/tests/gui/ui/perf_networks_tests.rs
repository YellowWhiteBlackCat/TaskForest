use super::super::perf_data::signal_quality_pct;
use super::*;

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
            taskmanager_application::NetworkAdapterType::WiFi
        } else {
            taskmanager_application::NetworkAdapterType::Unknown
        })
        .ssid_observation(match Some("Lab".into()) {
            Some(value) => taskmanager_application::OptionalObservation::present(value, 1),
            None => taskmanager_application::OptionalObservation::default(),
        })
        .signal_observation(match signal_dbm {
            Some(value) => taskmanager_application::OptionalObservation::present(value, 1),
            None => taskmanager_application::OptionalObservation::default(),
        })
        .build()
}

/// The RSSI→quality mapping matches the GPUI derivation exactly: -90 dBm
/// floors at 0%, -30 dBm ceilings at 100%, and the mid-range scales
/// linearly.
#[test]
fn signal_quality_pct_maps_the_standard_rssi_range() {
    let cases = [
        (-90, 0.0),
        (-75, 25.0),
        (-60, 50.0),
        (-52, 63.33),
        (-45, 75.0),
        (-30, 100.0),
    ];
    for (dbm, expected) in cases {
        let quality = signal_quality_pct(dbm);
        assert!(
            (quality - expected).abs() < 0.01,
            "signal {dbm} dBm should map to {expected}%, got {quality}%"
        );
    }
    // Out-of-range readings clamp to the honest bounds.
    assert_eq!(signal_quality_pct(-100), 0.0);
    assert_eq!(signal_quality_pct(-20), 100.0);
}

/// A wireless adapter with an observed signal renders the derived quality
/// percentage beside the dBm readout (GPUI parity); a wireless adapter
/// without a signal renders an honest dash and never invents a percentage.
#[test]
fn wireless_signal_renders_quality_only_from_an_observed_dbm() {
    with_english(|| {
        let history = LiveGraphHistory::default();
        let with_signal = network_lines(
            &[&wireless_network(Some(-52))],
            &history,
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
            &history,
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
            &taskmanager_application::SystemSnapshot {
                timestamp_ms,
                networks: vec![
                    taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
                        .device_id("network:test:eth0".into())
                        .interface_name("eth0".into())
                        .current_rx_bytes_per_sec(rx)
                        .current_tx_bytes_per_sec(tx)
                        .build(),
                ],
                ..taskmanager_application::SystemSnapshot::default()
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
        .device_generation(taskmanager_application::DeviceGeneration::new(1))
        .build();
    let lines = network_lines(&[&nic], &shell.history, TuiTheme::default(), true, true, 60);
    let receive_row = line_text(&lines[1]);
    let send_row = line_text(&lines[2]);
    assert!(
        receive_row.contains("Receive") && receive_row.contains("▁▅█"),
        "receive row must label its direction and span the shared ramp: {receive_row:?}"
    );
    assert!(
        send_row.contains("Send") && send_row.contains("███") && !send_row.contains('▅'),
        "send row must ride the shared maximum, not a per-row mid-ramp: {send_row:?}"
    );
    assert!(
        line_text(&lines[3]).contains("Throughput"),
        "the summed total summary stays under the direction pair"
    );
}
