//! Deterministic process, service, startup, and session inventory fixtures.

use super::*;
use taskmanager_core::core::process::{
    ProcessMetadataObservation, ProcessMetadataObservations, ProcessOwner, ProcessOwnerIdentity,
    ProcessScalarObservations,
};

pub(super) fn services() -> Vec<ServiceItem> {
    [
        (
            "NetworkManager.service",
            ServiceStatus::Active,
            "Network manager",
        ),
        (
            "bluetooth.service",
            ServiceStatus::Active,
            "Bluetooth service",
        ),
        (
            "docker.service",
            ServiceStatus::Inactive,
            "Container engine",
        ),
        (
            "systemd-timesyncd.service",
            ServiceStatus::Active,
            "Network time",
        ),
        (
            "demo-failed.service",
            ServiceStatus::Failed,
            "Recovery required",
        ),
    ]
    .into_iter()
    .map(|(name, status, description)| {
        ServiceItem::from_inventory(
            ServiceId::new(format!("fixture.service:{name}")),
            name,
            status,
            description,
            "",
            "",
            "",
        )
    })
    .collect()
}

pub(super) fn startup() -> Vec<StartupEntry> {
    vec![
        StartupEntry {
            id: "user-service:ssh-agent.service".into(),
            name: "SSH Agent".into(),
            exec: "ssh-agent.service".into(),
            enabled: true,
            source: StartupSource::UserService,
            scope: StartupScope::User,
            control_policy: StartupControlPolicy::Direct,
            locator: "ssh-agent.service".into(),
            impact: StartupImpact::Low,
            impact_evidence: StartupImpactEvidence::Measured { duration_ms: 42 },
        },
        StartupEntry {
            id: "desktop:clipboard-sync.desktop".into(),
            name: "Clipboard Sync".into(),
            exec: "wl-paste --watch".into(),
            enabled: true,
            source: StartupSource::DesktopEntry,
            scope: StartupScope::User,
            control_policy: StartupControlPolicy::Direct,
            locator: "clipboard-sync.desktop".into(),
            impact: StartupImpact::None,
            impact_evidence: StartupImpactEvidence::Unknown {
                reason: StartupImpactUnknownReason::NotInstrumented,
            },
        },
    ]
}

pub(super) fn sessions() -> Vec<SessionItem> {
    vec![
        SessionItem {
            id: "2".into(),
            uid: 1000,
            user: "devuser".into(),
            seat: Some("seat0".into()),
            tty: Some("tty2".into()),
            remote: false,
            timestamp: Some("2026-07-29 08:41".into()),
        },
        SessionItem {
            id: "9".into(),
            uid: 1000,
            user: "devuser".into(),
            seat: None,
            tty: Some("pts/4".into()),
            remote: true,
            timestamp: Some("2026-07-29 11:20".into()),
        },
    ]
}

pub(super) fn processes() -> Vec<ProcessItem> {
    [
        (4201, "zed", 24.8, 2_640, "devuser", "Running"),
        (1810, "gnome-shell", 9.6, 1_120, "devuser", "Running"),
        (9312, "rust-analyzer", 6.1, 842, "devuser", "Sleeping"),
        (1550, "Xwayland", 3.7, 378, "root", "Sleeping"),
        (8842, "cargo", 2.9, 244, "devuser", "Running"),
        (732, "NetworkManager", 1.1, 96, "root", "Sleeping"),
        (1, "systemd", 0.4, 18, "root", "Sleeping"),
        (9930, "taskmanager-tui", 0.3, 14, "devuser", "Running"),
        (843, "pipewire", 0.2, 42, "devuser", "Sleeping"),
        (712, "dbus-broker", 0.1, 12, "root", "Sleeping"),
        (602, "systemd-journald", 0.1, 64, "root", "Sleeping"),
        (77, "kworker/u64:2", 0.0, 0, "root", "Idle"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (pid, name, cpu, memory_mib, user, status))| {
        let start_time_secs = 1_785_290_000 + index as u64;
        let mut process = ProcessItem::new(pid, name);
        process.status = status.into();
        process.apply_metadata_observations(ProcessMetadataObservations {
            owner: ProcessMetadataObservation::available(
                ProcessOwner {
                    identity: ProcessOwnerIdentity::Opaque(user.into()),
                    label: None,
                },
                1,
            ),
            executable_path: ProcessMetadataObservation::absent(1),
        });
        process.apply_scalar_observations(ProcessScalarObservations {
            start_token: ScalarObservation::available(
                u64::from(pid) * 10_000 + index as u64 + 1,
                1,
            ),
            cpu_percentage: ScalarObservation::available(cpu, 1),
            memory_bytes: ScalarObservation::available(memory_mib * MIB, 1),
            start_time_secs: ScalarObservation::available(start_time_secs, 1),
            ..Default::default()
        });
        process
    })
    .collect()
}
