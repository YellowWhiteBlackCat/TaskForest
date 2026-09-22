//! Product-expected capability surface tests: identity coverage, canonical
//! wire ids, and the honest typed-absence descriptor.
//!
//! These pin the product-axis half of M3.2/M3.3: the expected identity set is a
//! product fact, an unregistered expected capability carries a typed absence
//! instead of disappearing, and unknown/vendor identities remain addressable
//! without panicking.

use taskmanager_core::ProviderId;
use taskmanager_platform_contract::{CapabilityDescriptor, CapabilityId, CapabilityStatus};

/// The product-expected wire ids. This census is deliberately written as
/// strings: it is the independent copy that catches a constant dropped from,
/// or added to, [`CapabilityId::EXPECTED_SURFACE`] without a conscious census
/// update.
const PRODUCT_CAPABILITY_CENSUS: [&str; 81] = [
    "accelerator.npu",
    "alerts.notify",
    "containers.rollup",
    "desktop.appearance",
    "desktop.responsiveness",
    "filesystem.deleted-handles",
    "filesystem.directory.usage",
    "filesystem.fd-limits",
    "filesystem.file-locks",
    "first-run.setup",
    "hardware.inventory",
    "hardware.power-supplies",
    "history.process-events",
    "ipc.dbus",
    "ipc.pipe-graph",
    "ipc.posix",
    "ipc.sysv",
    "memory.compression",
    "memory.hugepages",
    "memory.vma-map",
    "network.socket-control",
    "network.socket-inventory",
    "network.traffic-control",
    "numa.topology",
    "power.c-states",
    "power.profiles",
    "process.affinity",
    "process.affinity.control",
    "process.control",
    "process.insights.environment",
    "process.insights.gpu",
    "process.insights.isolation",
    "process.insights.network",
    "process.insights.open_files",
    "process.insights.resources",
    "process.insights.threads",
    "process.list",
    "process.network.escalation",
    "process.oom-score",
    "process.resource.control",
    "process.scheduling",
    "profiling.off-cpu",
    "profiling.pmu",
    "profiling.syscalls",
    "sensors",
    "services",
    "services.control",
    "services.dependencies",
    "services.logs",
    "services.logs.stream",
    "services.socket-activation",
    "services.timers",
    "sessions",
    "sessions.control",
    "shell.command.launch",
    "shell.resource.reveal",
    "shell.url.open",
    "startup",
    "startup.control",
    "startup.evidence",
    "storage.health",
    "storage.io-accounting",
    "storage.io-priority",
    "storage.smart",
    "storage.smart.control",
    "storage.writeback",
    "telemetry.cpu",
    "telemetry.cpu.msr",
    "telemetry.cpu.package_power",
    "telemetry.cpu.throttle",
    "telemetry.gpu",
    "telemetry.gpu.engines",
    "telemetry.gpu.hang",
    "telemetry.host",
    "telemetry.memory",
    "telemetry.memory.smbios",
    "telemetry.network",
    "telemetry.pressure",
    "telemetry.storage",
    "threads.context-switch",
    "threads.wait-channel",
];

fn expected_wire_ids() -> Vec<String> {
    CapabilityId::EXPECTED_SURFACE
        .iter()
        .map(|capability| capability.as_str().to_owned())
        .collect()
}

/// The expected surface is total, duplicate-free, and every wire id is a
/// canonical lowercase dotted slug: a typo cannot enter the ledger vocabulary.
#[test]
fn expected_surface_is_unique_and_canonically_named() {
    let ids = expected_wire_ids();
    assert_eq!(ids.len(), PRODUCT_CAPABILITY_CENSUS.len());

    let unique: std::collections::BTreeSet<_> = ids.iter().map(String::as_str).collect();
    assert_eq!(
        unique.len(),
        ids.len(),
        "the product surface must not contain duplicate identities"
    );

    for id in &ids {
        assert!(!id.is_empty(), "a capability identity cannot be empty");
        assert!(
            id.split('.').all(|segment| {
                !segment.is_empty()
                    && segment.bytes().all(|byte| {
                        byte.is_ascii_lowercase()
                            || byte.is_ascii_digit()
                            || byte == b'-'
                            || byte == b'_'
                    })
            }),
            "{id} must be a lowercase dotted slug"
        );
    }
}

/// The census and the contract constant agree in both directions: no expected
/// identity is unaccounted for and no census row is fabricated.
#[test]
fn expected_surface_matches_the_product_census() {
    let ids = expected_wire_ids();
    let census: std::collections::BTreeSet<_> = PRODUCT_CAPABILITY_CENSUS.into_iter().collect();
    let surface: std::collections::BTreeSet<_> = ids.iter().map(String::as_str).collect();
    assert_eq!(
        surface, census,
        "the expected surface drifted from the product census"
    );

    for id in ids {
        let capability = CapabilityId::owned(id);
        assert!(
            capability.is_expected(),
            "{capability} is part of the product surface"
        );
    }
}

/// Identity membership never fabricates availability and never panics on
/// unknown, malformed, or vendor identities.
#[test]
fn unknown_and_vendor_identities_are_outside_the_product_surface() {
    for unknown in [
        CapabilityId::owned("vendor.diagnostic"),
        CapabilityId::borrowed(""),
        CapabilityId::borrowed("Telemetry.Host"),
        CapabilityId::borrowed("telemetry host"),
        CapabilityId::borrowed("界"),
    ] {
        assert!(
            !unknown.is_expected(),
            "{unknown} must not be a product promise"
        );
    }
    assert!(CapabilityId::TELEMETRY_HOST.is_expected());
    assert!(CapabilityId::HISTORY_PROCESS_EVENTS.is_expected());
}

/// The typed absence is the single honest shape for an unregistered expected
/// capability: `Unsupported`, no provider attribution, no observation, no
/// fabricated last success - and it is distinguishable from a
/// registered-pending lane, which names its provider.
#[test]
fn typed_absence_is_honest_and_distinguishable_from_a_registered_lane() {
    let absence = CapabilityDescriptor::typed_absence(CapabilityId::PROCESS_LIST);
    assert_eq!(absence.status, CapabilityStatus::Unsupported);
    assert!(absence.providers.is_empty());
    assert_eq!(absence.observed_at_ms, 0);
    assert_eq!(absence.last_success_at_ms, None);
    assert!(absence.is_typed_absence());

    let registered_pending = CapabilityDescriptor {
        id: CapabilityId::TELEMETRY_CPU_MSR,
        status: CapabilityStatus::Unsupported,
        providers: vec![ProviderId::borrowed("macos.telemetry.cpu.msr")],
        observed_at_ms: 0,
        last_success_at_ms: None,
    };
    assert!(
        !registered_pending.is_typed_absence(),
        "a registered-pending lane carries a real provider identity"
    );

    let available = CapabilityDescriptor {
        id: CapabilityId::PROCESS_LIST,
        status: CapabilityStatus::Available,
        providers: vec![ProviderId::borrowed("fixture.process.list")],
        observed_at_ms: 1,
        last_success_at_ms: Some(1),
    };
    assert!(!available.is_typed_absence());
}

/// An expected surface entry is an identity only: manufacturing its typed
/// absence is always possible and never claims a provider or a value.
#[test]
fn every_expected_identity_can_answer_with_a_typed_absence() {
    for capability in CapabilityId::EXPECTED_SURFACE {
        let descriptor = CapabilityDescriptor::typed_absence(capability.clone());
        assert_eq!(descriptor.id, capability);
        assert!(descriptor.is_typed_absence(), "{capability}");
        assert!(descriptor.last_success_at_ms.is_none(), "{capability}");
    }
}
