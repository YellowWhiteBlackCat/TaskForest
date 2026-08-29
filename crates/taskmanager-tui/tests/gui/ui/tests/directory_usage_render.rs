//! Directory-usage projection panel render tests.
//!
//! The panel renders under the Disk device from the SHARED
//! `ShellData::directory_usage` slot (G-03: the scan lifecycle rides the
//! `PlatformEffect::DirectoryUsage` seam, and results fold into the shell
//! slot every frontend can read). These tests cover the render-only
//! projection: it DISPLAYS the shared snapshot and renders an honest idle
//! line when no projection has arrived. The scan lifecycle (start/cancel via
//! the `d` key) is covered by the runtime tests
//! (`runtime/tests/directory_usage.rs`). The snapshot types reach this
//! frontend through the core owner module (BN-01 vocabulary), so a headless
//! fixture snapshot can be
//! constructed and asserted against real `TestBackend` output.

use super::frame_text;

/// With no projection in the shared slot, the Disk device renders the
/// directory-usage panel's honest idle line — never fabricated entries,
/// totals, or a scan status.
#[test]
fn disk_device_directory_usage_renders_honest_idle_when_no_projection() {
    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Disk;
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::DirectoryUsage(None),
    );
    let text = frame_text(&app, 140, 48);
    assert!(
        text.contains("Directory usage"),
        "the panel title must render under the Disk device"
    );
    assert!(
        text.contains("No directory scan projected"),
        "an idle slot renders an honest line, not a fabricated panel"
    );
    // No fabricated scan data may render when the slot is empty.
    assert!(
        !text.contains("Status: Scanning") && !text.contains("Status: Completed"),
        "no fabricated scan status when no projection exists"
    );
    assert!(
        !text.contains("Totals:"),
        "no fabricated totals line when no projection exists"
    );
}

/// The directory-usage panel coexists with the per-disk detail: carving out its
/// area must not erase the disk's own telemetry. The demo disk's name still
/// renders alongside the directory-usage title.
#[test]
fn directory_usage_panel_does_not_displace_disk_detail() {
    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Disk;
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::DirectoryUsage(None),
    );
    let text = frame_text(&app, 140, 48);
    assert!(
        text.contains("Directory usage"),
        "directory-usage panel present"
    );
    // The demo snapshot carries one disk; its name must survive the area split.
    let snapshot = app
        .projection()
        .snapshot
        .as_ref()
        .expect("demo carries a snapshot");
    let disk_name = snapshot
        .disks
        .first()
        .map(|disk| disk.name.as_str())
        .expect("demo carries one disk");
    assert!(
        text.contains(disk_name),
        "disk detail must still render alongside the directory-usage panel"
    );
}

/// A projected shared snapshot renders its measured content: the scan root,
/// the readable entry's path on a row that also carries its measured base-2
/// size, the Debug status word, and the cumulative files/dirs counts. This
/// asserts real rendered frame text through the `TestBackend` — not
/// source-text matching (that would be 报菜名, banned).
#[test]
fn directory_usage_renders_measured_entries_status_and_totals_when_projected() {
    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Disk;
    // demo_app() seeds demo_directory_usage() into the shared slot; keep it
    // and assert against it.
    assert!(
        app.projection().directory_usage.is_some(),
        "demo seeds a shared directory-usage fixture"
    );
    let text = frame_text(&app, 140, 48);

    // The panel title and the scan root context line render.
    assert!(
        text.contains("Directory usage") && text.contains("Root: /var"),
        "panel title and scan root must render"
    );
    // The readable entry's row renders its path AND its measured size as a
    // base-2 bytes number (units default to bytes/base-2 in
    // `AppliedPrefs::default`); 2 GiB → "2.0 GiB".
    let readable_row = text
        .lines()
        .find(|line| line.contains("lib/postgres"))
        .expect("readable entry row must render");
    assert!(
        readable_row.contains("2.0 GiB"),
        "readable entry must render its measured size as a number: {readable_row:?}"
    );
    // The Debug status word renders (format!("{:?}") on DirectoryScanStatus).
    assert!(
        text.contains("Status: Completed"),
        "status Debug word must render: {text}"
    );
    // The cumulative files/dirs counts render in the Totals line.
    assert!(
        text.contains("Totals:") && text.contains("42 files") && text.contains("7 dirs"),
        "files/dirs totals must render"
    );
    // The capped flag and unreadable-directories count render.
    assert!(
        text.contains("capped") && text.contains("1 unreadable"),
        "capped + unreadable-directories counts must render"
    );
}

/// An unreadable subtree renders a danger dash — never a fabricated "0 B" and
/// never the untrustworthy measured size. The fixture seeds the unreadable
/// entry with a distinctive 7 GiB measured value; the renderer must suppress it
/// behind the dash (the measured-zero-vs-unavailable distinction is the point
/// of the typed `unreadable` flag). The assertion is row-precise so a "0 B/s"
/// throughput figure elsewhere in the disk detail cannot mask a fabrication.
#[test]
fn directory_usage_renders_dash_for_unreadable_not_fabricated_zero_or_size() {
    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Disk;
    assert!(
        app.projection().directory_usage.is_some(),
        "demo seeds a shared directory-usage fixture"
    );
    let text = frame_text(&app, 140, 48);

    // The unreadable entry's row renders (its row is present).
    let unreadable_row = text
        .lines()
        .find(|line| line.contains("cache/private"))
        .expect("unreadable entry row must render");
    // The row carries a danger dash for the unreadable subtree.
    assert!(
        unreadable_row.contains('—'),
        "unreadable entry must render a dash: {unreadable_row:?}"
    );
    // The untrustworthy 7 GiB measured value must NOT render on this row — the
    // dash supersedes the number for an unreadable subtree.
    assert!(
        !unreadable_row.contains("7.0 GiB"),
        "unreadable entry size must not leak as a number: {unreadable_row:?}"
    );
    // A fabricated measured zero ("0 B") must NOT render on this row either;
    // the typed `unreadable` flag and a confirmed empty directory are distinct.
    assert!(
        !unreadable_row.contains("0 B"),
        "unreadable entry must not render a fabricated '0 B': {unreadable_row:?}"
    );
}
