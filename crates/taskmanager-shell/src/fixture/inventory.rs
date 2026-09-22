//! Deterministic service, startup, and session inventory fixtures.

use super::*;

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
