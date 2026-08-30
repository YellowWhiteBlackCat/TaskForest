//! Deterministic process and startup fixtures used only by capture evidence.

use taskmanager_core::core::DeviceState;
use taskmanager_core::core::ScalarObservation;
use taskmanager_core::core::process::{
    ApplicationIconAsset, ApplicationIconFormat, ProcessApplicationIdentity, ProcessItem,
    ProcessLiveKey, ProcessMetadataObservation, ProcessMetadataObservations, ProcessOwner,
    ProcessOwnerIdentity, ProcessScalarObservations,
};
use taskmanager_core::core::startup::{
    BootTimeline, DEFAULT_BOOT_TIMELINE_MAX_SEGMENTS, DEFAULT_BOOT_TIMELINE_MAX_UNTIMED,
    StartupBootEvidenceSnapshot, StartupControlPolicy, StartupCriticalChainNode, StartupEntry,
    StartupFailedUnit, StartupImpact, StartupImpactEvidence, StartupImpactUnknownReason,
    StartupScope, StartupSource,
};

const CAPTURE_CHROME_ICON: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><circle cx="8" cy="8" r="7" fill="#4285f4"/><path d="M8 8h7A7 7 0 0 0 3 3z" fill="#ea4335"/><path d="M8 8 4.5 14A7 7 0 0 0 15 8z" fill="#fbbc05"/><circle cx="8" cy="8" r="3" fill="#34a853"/></svg>"##;
const CAPTURE_FIREFOX_ICON: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><circle cx="8" cy="8" r="7" fill="#ff7139"/><path d="M13 4c-2-2-5-2-7 0 1 0 2 1 2 2-2-1-4 0-5 2 0 3 2 5 5 5 3 0 5-2 5-5 0-2-1-3-2-4 1 0 2 0 2 0z" fill="#20123a"/></svg>"##;
const CAPTURE_EDITOR_ICON: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><rect x="2" y="2" width="12" height="12" rx="2" fill="#2ea043"/><path d="m5 10 2-2 1.5 1.5L11 7l1 1v3H5z" fill="#fff"/></svg>"##;

fn capture_application_identity(
    launcher_id: &str,
    display_name: &str,
    icon_token: &str,
    icon_bytes: &[u8],
) -> Option<ProcessApplicationIdentity> {
    let asset = ApplicationIconAsset::from_bytes(ApplicationIconFormat::Svg, icon_bytes.to_vec())?;
    ProcessApplicationIdentity::new(launcher_id, display_name, Some(icon_token.to_owned()))
        .map(|identity| identity.with_icon_resolution(Some(asset), None))
}

fn attach_capture_application_identity(
    process: &mut ProcessItem,
    identity: Option<ProcessApplicationIdentity>,
) {
    if let Some(identity) = identity {
        process.apply_application_identity(ProcessMetadataObservation::available(identity, 1));
    }
}

fn attach_capture_metadata(process: &mut ProcessItem, user: &str, executable: Option<&str>) {
    process.apply_metadata_observations(ProcessMetadataObservations {
        owner: ProcessMetadataObservation::available(
            ProcessOwner {
                identity: ProcessOwnerIdentity::Opaque(user.to_owned()),
                label: None,
            },
            1,
        ),
        executable_path: executable.map_or_else(
            || ProcessMetadataObservation::absent(1),
            |path| ProcessMetadataObservation::available(path.into(), 1),
        ),
    });
}

pub(super) fn prepare_process_tree(processes: &mut Vec<ProcessItem>) {
    processes.retain(|process| !(90_000..=90_006).contains(&process.pid));
    let mut root = ProcessItem::new(90_000, "capture-app");
    root.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(9_000_000, 1),
        ..Default::default()
    });
    processes.push(root);
    for offset in 1..=6 {
        let mut process = ProcessItem::new(90_000 + offset, format!("capture-worker-{offset}"));
        process.parent_pid = Some(if offset <= 2 {
            90_000
        } else {
            90_000 + offset - 2
        });
        process.apply_scalar_observations(ProcessScalarObservations {
            start_token: ScalarObservation::available(9_000_000 + u64::from(offset), 1),
            ..Default::default()
        });
        processes.push(process);
    }
}

pub(super) fn prepare_apps_group_expanded(processes: &mut Vec<ProcessItem>) {
    const BASE_PID: u32 = 93_201;
    processes.retain(|process| !(BASE_PID..BASE_PID.saturating_add(3)).contains(&process.pid));
    for (offset, (cpu, memory, status)) in [
        (32.0, 640 * 1024 * 1024, "Running"),
        (18.0, 384 * 1024 * 1024, "Sleeping"),
        (7.5, 192 * 1024 * 1024, "Sleeping"),
    ]
    .into_iter()
    .enumerate()
    {
        let pid = BASE_PID + u32::try_from(offset).unwrap_or(0);
        let threads = 6 + u32::try_from(offset).unwrap_or(0);
        let mut process = ProcessItem::new(pid, "capture-browser");
        process.cmdline = "/usr/bin/capture-browser --group-capture".into();
        process.status = status.into();
        process.apply_scalar_observations(ProcessScalarObservations {
            start_token: ScalarObservation::available(9_300_000 + u64::from(pid), 1),
            cpu_percentage: ScalarObservation::available(cpu, 1),
            memory_bytes: ScalarObservation::available(memory, 1),
            memory_pss_bytes: ScalarObservation::available(memory / 2, 1),
            disk_read_bytes_per_sec: ScalarObservation::available(16 * 1024, 1),
            disk_write_bytes_per_sec: ScalarObservation::available(8 * 1024, 1),
            threads: ScalarObservation::available(threads, 1),
            start_time_secs: ScalarObservation::available(
                1_703_000_000 + u64::try_from(offset).unwrap_or(0),
                1,
            ),
            ..ProcessScalarObservations::default()
        });
        process.cpu_history = vec![cpu - 5.0, cpu - 2.0, cpu];
        attach_capture_metadata(&mut process, "capture-user", None);
        attach_capture_application_identity(
            &mut process,
            capture_application_identity(
                "org.taskmanager.CaptureBrowser",
                "Capture Browser",
                "capture-browser",
                CAPTURE_CHROME_ICON,
            ),
        );
        processes.push(process);
    }
}

/// Prepare names that make the search-highlighting regression visible in a
/// compositor capture: the query matches one byte in each name, while the
/// surrounding characters are long enough to expose per-segment flex layout
/// failures. The fixture is presentation-only and uses a reserved PID range.
pub(super) fn prepare_apps_search_highlight(processes: &mut Vec<ProcessItem>) {
    const BASE_PID: u32 = 93_501;
    processes.retain(|process| !(BASE_PID..BASE_PID.saturating_add(3)).contains(&process.pid));
    for (offset, name) in [
        "taskforest-gui-long-process-name",
        "firefox-renderer",
        "systemd-festival-worker",
    ]
    .into_iter()
    .enumerate()
    {
        let mut process = ProcessItem::new(BASE_PID + u32::try_from(offset).unwrap_or(0), name);
        process.cmdline = format!("/usr/bin/{name}");
        process.status = "Sleeping".into();
        attach_capture_metadata(&mut process, "capture-user", None);
        processes.push(process);
    }
}

/// Prepare three high-signal application rows so a strict Apps capture can
/// show the provider-neutral identities for a PWA, a Snap launcher, and a
/// mounted AppImage in one frame. The executable paths/argv are fixture facts;
/// the real target-process proof remains in the Linux catalog tests and live
/// host receipt.
pub(super) fn prepare_apps_identity_matrix(processes: &mut Vec<ProcessItem>) {
    const BASE_PID: u32 = 93_401;
    processes.retain(|process| !(BASE_PID..BASE_PID.saturating_add(3)).contains(&process.pid));
    for (
        offset,
        (
            name,
            cmdline,
            exe_path,
            launcher_id,
            display_name,
            icon_token,
            icon_bytes,
            cpu,
            memory,
            status,
        ),
    ) in [
        (
            "chrome",
            "/opt/google/chrome/chrome --profile-directory=Default --app-id=abc123",
            "/opt/google/chrome/chrome",
            "chrome-mail.desktop",
            "Mail PWA",
            "chrome-mail",
            CAPTURE_CHROME_ICON,
            96.0,
            768 * 1024 * 1024,
            "Running",
        ),
        (
            "snap",
            "/usr/bin/snap run firefox",
            "/usr/bin/snap",
            "snap.firefox_firefox.desktop",
            "Firefox (Snap)",
            "firefox",
            CAPTURE_FIREFOX_ICON,
            84.0,
            512 * 1024 * 1024,
            "Sleeping",
        ),
        (
            "AppRun",
            "/tmp/.mount_PortableEditor-abc123/AppRun",
            "/tmp/.mount_PortableEditor-abc123/AppRun",
            "portable-editor.desktop",
            "Portable Editor (AppImage)",
            "portable-editor",
            CAPTURE_EDITOR_ICON,
            72.0,
            384 * 1024 * 1024,
            "Running",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let pid = BASE_PID + u32::try_from(offset).unwrap_or(0);
        let threads = 4 + u32::try_from(offset).unwrap_or(0);
        let mut process = ProcessItem::new(pid, name);
        process.cmdline = cmdline.into();
        process.status = status.into();
        process.apply_scalar_observations(ProcessScalarObservations {
            start_token: ScalarObservation::available(9_340_000 + u64::from(pid), 1),
            cpu_percentage: ScalarObservation::available(cpu, 1),
            memory_bytes: ScalarObservation::available(memory, 1),
            memory_pss_bytes: ScalarObservation::available(memory / 2, 1),
            swap_bytes: ScalarObservation::available(32 * 1024 * 1024, 1),
            disk_read_bytes_per_sec: ScalarObservation::available(24 * 1024, 1),
            disk_write_bytes_per_sec: ScalarObservation::available(12 * 1024, 1),
            threads: ScalarObservation::available(threads, 1),
            start_time_secs: ScalarObservation::available(
                1_704_000_000 + u64::try_from(offset).unwrap_or(0),
                1,
            ),
            ..ProcessScalarObservations::default()
        });
        process.cpu_history = vec![cpu - 8.0, cpu - 3.0, cpu];
        attach_capture_metadata(&mut process, "capture-user", Some(exe_path));
        attach_capture_application_identity(
            &mut process,
            capture_application_identity(launcher_id, display_name, icon_token, icon_bytes),
        );
        processes.push(process);
    }
}

pub(super) fn prepare_process_batch(processes: &mut Vec<ProcessItem>) {
    processes.retain(|process| !(91_001..=91_003).contains(&process.pid));
    for (pid, name, start_time_secs) in [
        (91_001, "capture-renderer", 1_701_000_001),
        (91_002, "capture-indexer", 1_701_000_002),
        (91_003, "capture-worker", 1_701_000_003),
    ] {
        let mut process = ProcessItem::new(pid, name);
        process.apply_scalar_observations(ProcessScalarObservations {
            start_token: ScalarObservation::available(u64::from(pid) * 100, 1),
            start_time_secs: ScalarObservation::available(start_time_secs, 1),
            ..Default::default()
        });
        process.status = "Sleeping".into();
        attach_capture_metadata(&mut process, "capture-user", None);
        processes.push(process);
    }
}

pub(super) fn prepare_process_memory_pss_swap(processes: &mut Vec<ProcessItem>) {
    const BASE_PID: u32 = 93_001;
    processes.retain(|process| !(BASE_PID..BASE_PID.saturating_add(4)).contains(&process.pid));
    for (offset, (name, user, rss, pss, swap, cpu, status)) in [
        (
            "capture-browser",
            "capture-user",
            768 * 1024 * 1024,
            410 * 1024 * 1024,
            96 * 1024 * 1024,
            38.0,
            "Running",
        ),
        (
            "capture-editor",
            "capture-user",
            512 * 1024 * 1024,
            292 * 1024 * 1024,
            0,
            21.5,
            "Sleeping",
        ),
        (
            "capture-worker",
            "capture-service",
            256 * 1024 * 1024,
            144 * 1024 * 1024,
            32 * 1024 * 1024,
            8.5,
            "Running",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let pid = BASE_PID + u32::try_from(offset).unwrap_or(0);
        let threads = 4 + u32::try_from(offset).unwrap_or(0);
        let mut process = ProcessItem::new(pid, name);
        process.cmdline = format!("/usr/bin/{name} --capture");
        process.status = status.into();
        process.apply_scalar_observations(ProcessScalarObservations {
            start_token: ScalarObservation::available(8_000_000 + u64::from(pid), 1),
            cpu_percentage: ScalarObservation::available(cpu, 1),
            memory_bytes: ScalarObservation::available(rss, 1),
            memory_pss_bytes: ScalarObservation::available(pss, 1),
            swap_bytes: ScalarObservation::available(swap, 1),
            disk_read_bytes_per_sec: ScalarObservation::available(12 * 1024, 1),
            disk_write_bytes_per_sec: ScalarObservation::available(8 * 1024, 1),
            threads: ScalarObservation::available(threads, 1),
            ..ProcessScalarObservations::default()
        });
        process.cpu_history = vec![cpu - 4.0, cpu - 1.0, cpu];
        attach_capture_metadata(&mut process, user, None);
        processes.push(process);
    }
}

pub(super) fn prepare_apps_zero_gray(processes: &mut Vec<ProcessItem>) {
    const BASE_PID: u32 = 93_101;
    processes.retain(|process| !(BASE_PID..BASE_PID.saturating_add(2)).contains(&process.pid));
    for (offset, (name, user, status)) in [
        ("capture-idle", "capture-user", "Sleeping"),
        ("capture-helper", "capture-service", "Running"),
    ]
    .into_iter()
    .enumerate()
    {
        let pid = BASE_PID + u32::try_from(offset).unwrap_or(0);
        let threads = 1 + u32::try_from(offset).unwrap_or(0);
        let mut process = ProcessItem::new(pid, name);
        process.cmdline = format!("/usr/bin/{name} --zero-gray-capture");
        process.status = status.into();
        process.apply_scalar_observations(ProcessScalarObservations {
            start_token: ScalarObservation::available(9_000_000 + u64::from(pid), 1),
            cpu_percentage: ScalarObservation::available(0.0, 1),
            memory_bytes: ScalarObservation::available(0, 1),
            memory_pss_bytes: ScalarObservation::available(0, 1),
            swap_bytes: ScalarObservation::available(0, 1),
            disk_read_bytes_per_sec: ScalarObservation::available(0, 1),
            disk_write_bytes_per_sec: ScalarObservation::available(0, 1),
            threads: ScalarObservation::available(threads, 1),
            ..ProcessScalarObservations::default()
        });
        attach_capture_metadata(&mut process, user, None);
        let (launcher_id, display_name, icon_token) = if name == "capture-idle" {
            (
                "org.taskmanager.CaptureIdle",
                "Capture Idle",
                "capture-idle",
            )
        } else {
            (
                "org.taskmanager.CaptureHelper",
                "Capture Helper",
                "capture-helper",
            )
        };
        attach_capture_application_identity(
            &mut process,
            capture_application_identity(
                launcher_id,
                display_name,
                icon_token,
                if name == "capture-idle" {
                    CAPTURE_FIREFOX_ICON
                } else {
                    CAPTURE_EDITOR_ICON
                },
            ),
        );
        processes.push(process);
    }
}

pub(super) fn prepare_process_insights(processes: &mut Vec<ProcessItem>) -> Option<ProcessLiveKey> {
    const PID: u32 = 4242;
    processes.retain(|process| process.pid != PID);
    let mut process = ProcessItem::new(PID, "capture-telemetry-worker");
    process.parent_pid = Some(1);
    process.cmdline = "/usr/bin/capture-telemetry-worker --isolated".into();
    process.status = "Running".into();
    process.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(987_654, 1),
        cpu_percentage: ScalarObservation::available(37.5, 1),
        memory_bytes: ScalarObservation::available(384 * 1024 * 1024, 1),
        threads: ScalarObservation::available(7, 1),
        start_time_secs: ScalarObservation::available(1_703_000_001, 1),
        ..Default::default()
    });
    attach_capture_metadata(&mut process, "capture-user", None);
    processes.push(process);
    ProcessLiveKey::from_parts(PID, 987_654)
}

pub(super) fn prepare_diagnostic_process(processes: &mut Vec<ProcessItem>) {
    processes.retain(|process| process.pid != 92_001);
    let mut process = ProcessItem::new(92_001, "capture-private-worker");
    process.cmdline = "/home/<user>/bin/private --peer 10.77.0.9".into();
    process.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(9_200_100, 1),
        start_time_secs: ScalarObservation::available(1_702_000_001, 1),
        ..Default::default()
    });
    process.status = "Sleeping".into();
    attach_capture_metadata(&mut process, "capture-user", None);
    processes.push(process);
}

pub(super) fn prepare_startup_impact(entries: &mut Vec<StartupEntry>) {
    entries.clear();
    entries.extend([
        StartupEntry {
            id: "user-service:capture-sync.service".into(),
            name: "Measured sync service".into(),
            exec: "capture-sync.service".into(),
            enabled: true,
            source: StartupSource::UserService,
            scope: StartupScope::User,
            control_policy: StartupControlPolicy::Direct,
            locator: "capture-sync.service".into(),
            impact: StartupImpact::High,
            impact_evidence: StartupImpactEvidence::Measured { duration_ms: 842 },
        },
        StartupEntry {
            id: "desktop:helper.desktop".into(),
            name: "Desktop helper".into(),
            exec: "capture-desktop-helper --background".into(),
            enabled: true,
            source: StartupSource::DesktopEntry,
            scope: StartupScope::User,
            control_policy: StartupControlPolicy::Direct,
            locator: "/home/<user>/.config/autostart/helper.desktop".into(),
            impact: StartupImpact::None,
            impact_evidence: StartupImpactEvidence::Unknown {
                reason: StartupImpactUnknownReason::NotInstrumented,
            },
        },
    ]);
}

/// Deterministic Startup evidence fixture for the real failure presentation:
/// three failed units plus a measured critical chain. This keeps the red
/// failed-unit strip and the waterfall covered without depending on the host's
/// current systemd state.
pub(super) fn prepare_startup_failure_evidence(
    entries: &mut Vec<StartupEntry>,
    evidence: &mut Option<StartupBootEvidenceSnapshot>,
    baseline: &mut Option<BootTimeline>,
) {
    prepare_startup_impact(entries);
    *baseline = None;
    *evidence = Some(StartupBootEvidenceSnapshot {
        state: DeviceState::healthy(10),
        failed_units_state: DeviceState::healthy(10),
        critical_chain_state: DeviceState::healthy(10),
        failed_units_failure: None,
        critical_chain_failure: None,
        failed_units: vec![
            failed_unit("taskforest-g.service"),
            failed_unit("taskforest-i.service"),
            failed_unit("taskforest.service"),
        ],
        critical_chain: vec![
            chain_node("dbus.socket", 0, 6),
            chain_node("graphical-session.target", 6, 0),
        ],
    });
}

fn failed_unit(unit: &str) -> StartupFailedUnit {
    StartupFailedUnit {
        unit: unit.to_owned(),
        load_state: "loaded".into(),
        active_state: "failed".into(),
        sub_state: "failed".into(),
        description: "capture fixture failed unit".into(),
    }
}

pub(super) fn prepare_process_histories(process: &mut ProcessItem) {
    process.cpu_history = (0..60).map(|i| 12.0 + (i % 12) as f32 * 2.0).collect();
    process.mem_history = (0..60)
        .map(|i| 240_000_000.0 + (i % 15) as f32 * 8_000_000.0)
        .collect();
    process.disk_read_history = (0..60).map(|i| (i % 10) as f32 * 900_000.0).collect();
    process.disk_write_history = (0..60).map(|i| (i % 8) as f32 * 500_000.0).collect();
}

/// Startup waterfall + roadmap #5 comparison-markers fixture: keeps the
/// impact list, then seeds the CURRENT boot's critical chain and a
/// previous-boot baseline over the same units with shifted durations, so the
/// per-unit delta chips show all three honest states — slower (danger),
/// faster (success), unchanged (dim). Presentation-only: nothing here is
/// recorded into a real boot history (this scenario runs with persistence
/// off; the replay scenario owns the on-disk fixtures).
pub(super) fn prepare_startup_boot_markers(
    entries: &mut Vec<StartupEntry>,
    evidence: &mut Option<StartupBootEvidenceSnapshot>,
    baseline: &mut Option<BootTimeline>,
) {
    prepare_startup_impact(entries);
    // Current boot: capture-sync 842ms, graphical 980ms, network-online 510ms.
    let current_chain = [
        chain_node("network-online.target", 260, 510),
        chain_node("capture-sync.service", 400, 842),
        chain_node("graphical.target", 1_600, 980),
    ];
    *evidence = Some(StartupBootEvidenceSnapshot {
        state: DeviceState::healthy(10),
        failed_units_state: DeviceState::healthy(10),
        critical_chain_state: DeviceState::healthy(10),
        failed_units_failure: None,
        critical_chain_failure: None,
        failed_units: Vec::new(),
        critical_chain: current_chain.to_vec(),
    });
    // Previous boot over the SAME units: capture-sync faster (-200),
    // graphical slower (+300), network-online identical (0).
    let previous_chain = [
        chain_node("network-online.target", 250, 510),
        chain_node("capture-sync.service", 380, 642),
        chain_node("graphical.target", 1_540, 1_280),
    ];
    *baseline = Some(BootTimeline::from_critical_chain(
        &previous_chain,
        DEFAULT_BOOT_TIMELINE_MAX_SEGMENTS,
        DEFAULT_BOOT_TIMELINE_MAX_UNTIMED,
    ));
}

fn chain_node(unit: &str, activated_at_ms: u64, duration_ms: u64) -> StartupCriticalChainNode {
    StartupCriticalChainNode {
        unit: unit.to_owned(),
        activated_at_ms: Some(activated_at_ms),
        duration_ms: Some(duration_ms),
    }
}
