use super::*;
use taskmanager_application::SystemSnapshot;

/// A disk whose throughput history has >=2 samples renders a real sparkline
/// (a ramp block) on its trend line; a disk with no history renders the
/// dotted placeholder. The trend line sits right after the header (index 1).
#[test]
fn disk_trend_line_matches_that_disks_own_history_window() {
    // Record two snapshots for "sda" so its window has >=2 samples.
    let mut shell = taskmanager_shell::ShellApp::new();
    let snapshot = SystemSnapshot {
        disks: vec![
            taskmanager_test_support::DiskMetricsFixtureBuilder::new()
                .device_id("disk:test:sda".into())
                .name("sda".into())
                .current_read_bytes_per_sec(1_048_576)
                .current_write_bytes_per_sec(1_048_576)
                .build(),
        ],
        ..SystemSnapshot::default()
    };
    taskmanager_shell::fixture::record_demo_history_frame(&mut shell, &snapshot, None, None);
    taskmanager_shell::fixture::record_demo_history_frame(&mut shell, &snapshot, None, None);
    let history = &shell.history;
    // A constant throughput window resolves and trends to a flat mid-ramp.
    // The demo seeding resets the ring for generation 1, so the probe reads
    // at that generation like a bound projection row would.
    let window = history.disk_bytes_per_sec_for("disk:test:sda", 1);
    assert_eq!(window.len(), 2, "two snapshots recorded");
    assert!(
        super::super::sparkline::test_support::device_trend(&window).contains('▅'),
        "constant throughput → flat mid-ramp"
    );

    // The trend line in disk_lines is line index 1 (right after the header).
    // The row must carry the generation its ring was reset for — a bound
    // platform row always does; an unbound 0 renders no curve by contract.
    let known = disk_lines(
        &[taskmanager_test_support::DiskMetricsFixtureBuilder::new()
            .device_id("disk:test:sda".into())
            .name("sda".into())
            .device_generation(taskmanager_application::DeviceGeneration::new(1))
            .build()],
        history,
        TuiTheme::default(),
        true,
        true,
        60,
    );
    let trend_text: String = known[1]
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    assert!(
        trend_text.contains('▅'),
        "known disk renders a ramp-block trend: {trend_text:?}"
    );

    // A disk the history has never seen renders the dotted placeholder — a
    // bound row (gen 1) with no accepted ring, so the assertion stays about
    // identity absence, not an unbound generation.
    let cold = disk_lines(
        &[taskmanager_test_support::DiskMetricsFixtureBuilder::new()
            .device_id("disk:test:nvme1".into())
            .name("nvme1n1".into())
            .device_generation(taskmanager_application::DeviceGeneration::new(1))
            .build()],
        history,
        TuiTheme::default(),
        true,
        true,
        60,
    );
    let cold_trend: String = cold[1]
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    assert!(
        cold_trend.contains('·'),
        "unknown disk renders the placeholder: {cold_trend:?}"
    );
}

#[test]
fn smart_temperature_trend_never_mixes_other_disks_history() {
    let mut shell = taskmanager_shell::ShellApp::new();
    for (timestamp_ms, temperature_a, temperature_b) in [(1_u64, 31.0, 61.0), (2_u64, 33.0, 63.0)] {
        let disk = |device_id: &str, name: &str, temperature_c| {
            taskmanager_test_support::DiskMetricsFixtureBuilder::new()
                .device_id(device_id.to_owned())
                .name(name.to_owned())
                .smart_availability(taskmanager_application::SmartAvailability::Available)
                .smart_state(taskmanager_application::DeviceState::healthy(timestamp_ms))
                .smart_temperature_c(Some(temperature_c))
                .build()
        };
        taskmanager_shell::fixture::record_demo_history_frame(
            &mut shell,
            &SystemSnapshot {
                timestamp_ms,
                disks: vec![
                    disk("disk:test:temperature-a", "disk-a", temperature_a),
                    disk("disk:test:temperature-b", "disk-b", temperature_b),
                ],
                ..SystemSnapshot::default()
            },
            None,
            None,
        );
    }

    let selected = taskmanager_test_support::DiskMetricsFixtureBuilder::new()
        .device_id("disk:test:temperature-a".into())
        .name("disk-a".into())
        .device_generation(taskmanager_application::DeviceGeneration::new(1))
        .smart_availability(taskmanager_application::SmartAvailability::Available)
        .smart_temperature_c(Some(33.0))
        .build();
    let text = disk_lines(
        &[selected],
        &shell.history,
        TuiTheme::default(),
        true,
        true,
        60,
    )
    .iter()
    .flat_map(|line| line.spans.iter())
    .map(|span| span.content.as_ref())
    .collect::<String>();
    assert!(
        text.contains("33°C"),
        "selected disk's latest sample is shown"
    );
    assert!(text.contains("32°C"), "selected disk's average is shown");
    assert!(
        !text.contains("63°C") && !text.contains("47°C"),
        "disk B's latest or a cross-device average must never enter disk A's detail trend"
    );
}

/// Flatten a ratatui `Line` back to its raw text so a test can assert on
/// the rendered string.
fn line_text(line: &ratatui::text::Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

/// One `DiskMetrics` for "sda" carrying only the direction rates the frame
/// actually observed — an unset direction stays an unavailable scalar so the
/// store records an explicit `NaN` gap for it.
fn rate_disk(read: Option<u64>, write: Option<u64>) -> DiskMetrics {
    // Enter the named-override stage with an all-unavailable scalar group so
    // the two conditional setters below share one builder type. The demo
    // seeding resets rings for generation 1, so the rendered row carries that
    // bound generation like a real platform row would.
    let builder = taskmanager_test_support::DiskMetricsFixtureBuilder::new()
        .device_id("disk:test:sda".into())
        .name("sda".into())
        .device_generation(taskmanager_application::DeviceGeneration::new(1))
        .scalar_observations(Default::default());
    let builder = match read {
        Some(value) => builder.current_read_bytes_per_sec(value),
        None => builder,
    };
    match write {
        Some(value) => builder.current_write_bytes_per_sec(value),
        None => builder,
    }
    .build()
}

/// Record `disk` as one history frame at `timestamp_ms` through the same
/// correlated ingestor the live collector uses.
fn record_frame(shell: &mut taskmanager_shell::ShellApp, timestamp_ms: u64, disk: DiskMetrics) {
    taskmanager_shell::fixture::record_demo_history_frame(
        shell,
        &SystemSnapshot {
            timestamp_ms,
            disks: vec![disk],
            ..SystemSnapshot::default()
        },
        None,
        None,
    );
}

// test-intent: behavior
/// The disk block's two direction rows project this disk's OWN split windows
/// on one shared scale: the rising read spans the full ramp while the
/// constant write — pinned at the pair's maximum — rides the top block
/// (per-row normalization would paint it mid-ramp), and the summed
/// Throughput summary stays beneath the pair.
#[test]
fn disk_direction_rows_share_one_scale_and_keep_the_summed_summary() {
    taskmanager_test_support::pin_english();
    let mut shell = taskmanager_shell::ShellApp::new();
    record_frame(&mut shell, 1, rate_disk(Some(1_048_576), Some(3_145_728)));
    record_frame(&mut shell, 2, rate_disk(Some(2_097_152), Some(3_145_728)));
    record_frame(&mut shell, 3, rate_disk(Some(3_145_728), Some(3_145_728)));

    let lines = disk_lines(
        &[rate_disk(None, None)],
        &shell.history,
        TuiTheme::default(),
        true,
        true,
        60,
    );
    let read_row = line_text(&lines[1]);
    let write_row = line_text(&lines[2]);
    assert!(
        read_row.contains("Read") && read_row.contains("▁▅█"),
        "read row must label its direction and span the shared ramp: {read_row:?}"
    );
    assert!(
        write_row.contains("Write") && write_row.contains("███") && !write_row.contains('▅'),
        "write row must ride the shared maximum, not a per-row mid-ramp: {write_row:?}"
    );
    assert!(
        line_text(&lines[3]).contains("Throughput"),
        "the summed total summary stays under the direction pair"
    );
}

// test-intent: behavior
/// A direction whose samples went missing mid-window renders gap glyphs in
/// its row while its companion keeps a clean ramp — the split windows carry
/// their own per-direction gaps, never a fabricated zero block.
#[test]
fn disk_direction_rows_render_per_direction_gaps() {
    taskmanager_test_support::pin_english();
    let mut shell = taskmanager_shell::ShellApp::new();
    // Frames 1-2: write-only (read scalar unavailable → NaN gaps). Frames
    // 3-4: read joins at 1 then 2 MiB/s while write stays at 2 MiB/s.
    record_frame(&mut shell, 1, rate_disk(None, Some(2_097_152)));
    record_frame(&mut shell, 2, rate_disk(None, Some(2_097_152)));
    record_frame(&mut shell, 3, rate_disk(Some(1_048_576), Some(2_097_152)));
    record_frame(&mut shell, 4, rate_disk(Some(2_097_152), Some(2_097_152)));

    let lines = disk_lines(
        &[rate_disk(None, None)],
        &shell.history,
        TuiTheme::default(),
        true,
        true,
        60,
    );
    let read_row = line_text(&lines[1]);
    let write_row = line_text(&lines[2]);
    assert!(
        read_row.contains("Read") && read_row.contains('·'),
        "read row must show its explicit gaps: {read_row:?}"
    );
    assert!(
        !read_row.contains("····"),
        "a direction with two finite samples is live, not collecting: {read_row:?}"
    );
    assert!(
        write_row.contains("Write") && !write_row.contains('·'),
        "the fully observed companion must stay gap-free: {write_row:?}"
    );
}

// test-intent: behavior
/// The active-time row projects this disk's OWN activity window: a rising
/// window spans the per-row ramp, the absolute 0-100 statistics ride in the
/// summary beneath, and a disk with no recorded activity keeps the dotted
/// collecting placeholder instead of a fabricated flat trend or summary.
#[test]
fn disk_active_time_row_trends_its_own_window_with_percent_summary() {
    taskmanager_test_support::pin_english();
    let mut shell = taskmanager_shell::ShellApp::new();
    let active_disk = |active_pct: f32| {
        taskmanager_test_support::DiskMetricsFixtureBuilder::new()
            .device_id("disk:test:sda".into())
            .name("sda".into())
            .device_generation(taskmanager_application::DeviceGeneration::new(1))
            .current_read_bytes_per_sec(1_048_576)
            .current_write_bytes_per_sec(1_048_576)
            .current_active_time_pct(active_pct)
            .build()
    };
    record_frame(&mut shell, 1, active_disk(10.0));
    record_frame(&mut shell, 2, active_disk(50.0));
    record_frame(&mut shell, 3, active_disk(90.0));

    let known = disk_lines(
        &[active_disk(90.0)],
        &shell.history,
        TuiTheme::default(),
        true,
        true,
        60,
    );
    let texts: Vec<String> = known.iter().map(line_text).collect();
    let active_row = texts
        .iter()
        .find(|text| text.trim_start().starts_with("Active time"))
        .expect("recorded disk renders its active-time trend row");
    assert!(
        active_row.contains("▁▅█"),
        "rising activity spans the per-row ramp: {active_row:?}"
    );
    let summary = texts
        .iter()
        .find(|text| text.contains("Active time · Latest"))
        .expect("active-time summary rides beneath the trend row");
    assert!(
        summary.contains("90%") && summary.contains("50%"),
        "the summary carries the absolute 0-100 statistics: {summary:?}"
    );

    // A disk the activity ring has never seen keeps the dotted placeholder
    // and no fabricated percent summary — a bound row (gen 1) with no ring.
    let cold = disk_lines(
        &[taskmanager_test_support::DiskMetricsFixtureBuilder::new()
            .device_id("disk:test:nvme-cold".into())
            .name("nvme-cold".into())
            .device_generation(taskmanager_application::DeviceGeneration::new(1))
            .build()],
        &shell.history,
        TuiTheme::default(),
        true,
        true,
        60,
    );
    let cold_texts: Vec<String> = cold.iter().map(line_text).collect();
    let cold_row = cold_texts
        .iter()
        .find(|text| text.trim_start().starts_with("Active time"))
        .expect("cold disk still renders its active-time row");
    assert!(
        cold_row.contains("····") && !cold_row.contains('█'),
        "unknown disk renders the collecting placeholder: {cold_row:?}"
    );
    assert!(
        !cold_texts
            .iter()
            .any(|text| text.contains("Active time · Latest")),
        "no percent summary without a finite sample"
    );
}
