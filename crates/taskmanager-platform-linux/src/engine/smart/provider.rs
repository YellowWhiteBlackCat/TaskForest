//! Runtime SMART protocol provider registry.
//!
//! The standard Linux artifact registers every protocol family together.
//! Transport evidence selects a provider at runtime; provider names are
//! diagnostics metadata and never user-facing build variants.

use std::collections::BTreeMap;

use taskmanager_core::core::metrics::{
    SmartAvailability, StorageConnection, StorageDeviceKind, StorageInterconnect, StorageProtocol,
};
use taskmanager_core::{
    DiskSmart, FailureKind, ProviderId, SmartProviderFailureKind, SourceOutcome, SourceStatus,
};

use super::transport::read_smartctl_smart;
use super::{nvme_controller_from_name, read_nvme_smart};

struct SmartDeviceRequest<'a> {
    name: &'a str,
    connection: StorageConnection,
}

trait SmartTelemetryProvider: Send + Sync {
    fn id(&self) -> ProviderId;
    fn supports(&self, request: &SmartDeviceRequest<'_>) -> bool;
    fn observe(&self, request: &SmartDeviceRequest<'_>) -> DiskSmart;
}

/// Protocol registry shared by every collector refresh.
pub(crate) struct SmartProviderRegistry {
    providers: Vec<Box<dyn SmartTelemetryProvider>>,
}

pub(crate) struct SmartRegistryObservation {
    pub(crate) value: DiskSmart,
    pub(crate) source: SourceStatus,
}

impl SmartProviderRegistry {
    pub(crate) fn standard() -> Self {
        let mut registry = Self {
            providers: Vec::new(),
        };
        registry.register(NvmeSmartProvider);
        registry.register(AtaSmartProvider);
        registry.register(ScsiSmartProvider);
        registry.register(UsbBridgeSmartProvider);
        registry.register(AutoDetectSmartProvider);
        registry
    }

    fn register(&mut self, provider: impl SmartTelemetryProvider + 'static) {
        self.providers.push(Box::new(provider));
    }

    pub(crate) fn observe(
        &self,
        name: &str,
        connection: StorageConnection,
    ) -> SmartRegistryObservation {
        let request = SmartDeviceRequest { name, connection };
        let Some(provider) = self
            .providers
            .iter()
            .find(|provider| provider.supports(&request))
        else {
            let value = DiskSmart::with_failure(SmartProviderFailureKind::UnsupportedProtocol);
            return SmartRegistryObservation {
                source: smart_source_status(ProviderId::borrowed("linux.smart.registry"), &value),
                value,
            };
        };
        let provider_id = provider.id();
        let mut observation = provider.observe(&request);
        observation.provider = Some(provider_id.clone());
        SmartRegistryObservation {
            source: smart_source_status(provider_id, &observation),
            value: observation,
        }
    }
}

fn smart_source_status(provider: ProviderId, observation: &DiskSmart) -> SourceStatus {
    let failure = observation
        .failure
        .map(smart_failure_kind)
        .or(match observation.availability {
            SmartAvailability::Available => None,
            SmartAvailability::Unsupported => Some(FailureKind::Unsupported),
            SmartAvailability::Unavailable => Some(FailureKind::TemporarilyUnavailable),
            SmartAvailability::MissingTool => Some(FailureKind::MissingDependency),
            SmartAvailability::PermissionDenied => Some(FailureKind::PermissionDenied),
        });
    let item_count = usize::from(observation.availability == SmartAvailability::Available);
    SourceStatus {
        provider,
        outcome: match (item_count, failure) {
            (_, None) => SourceOutcome::Available,
            (0, Some(failure)) => SourceOutcome::Unavailable(failure),
            (_, Some(failure)) => SourceOutcome::Partial(failure),
        },
        item_count,
    }
}

pub(crate) fn aggregate_smart_sources(
    sources: impl IntoIterator<Item = SourceStatus>,
) -> Vec<SourceStatus> {
    #[derive(Default)]
    struct Aggregate {
        successful: usize,
        failure: Option<FailureKind>,
    }

    let mut by_provider = BTreeMap::<ProviderId, Aggregate>::new();
    for source in sources {
        let aggregate = by_provider.entry(source.provider).or_default();
        aggregate.successful = aggregate.successful.saturating_add(source.item_count);
        let failure = match source.outcome {
            SourceOutcome::Available | SourceOutcome::Empty => None,
            SourceOutcome::Partial(failure) | SourceOutcome::Unavailable(failure) => Some(failure),
        };
        if let Some(failure) = failure
            && aggregate.failure.is_none_or(|current| {
                smart_failure_priority(failure) > smart_failure_priority(current)
            })
        {
            aggregate.failure = Some(failure);
        }
    }
    by_provider
        .into_iter()
        .map(|(provider, aggregate)| SourceStatus {
            provider,
            outcome: match (aggregate.successful, aggregate.failure) {
                (0, None) => SourceOutcome::Empty,
                (_, None) => SourceOutcome::Available,
                (0, Some(failure)) => SourceOutcome::Unavailable(failure),
                (_, Some(failure)) => SourceOutcome::Partial(failure),
            },
            item_count: aggregate.successful,
        })
        .collect()
}

const fn smart_failure_kind(failure: SmartProviderFailureKind) -> FailureKind {
    match failure {
        SmartProviderFailureKind::UnsupportedProtocol
        | SmartProviderFailureKind::BridgeLimitation => FailureKind::Unsupported,
        SmartProviderFailureKind::MissingTool => FailureKind::MissingDependency,
        SmartProviderFailureKind::PermissionDenied => FailureKind::PermissionDenied,
        SmartProviderFailureKind::TimedOut => FailureKind::TimedOut,
        SmartProviderFailureKind::MalformedResponse | SmartProviderFailureKind::CommandFailed => {
            FailureKind::ProviderFault
        }
        SmartProviderFailureKind::DeviceUnavailable
        | SmartProviderFailureKind::TemporarilyUnavailable => FailureKind::TemporarilyUnavailable,
    }
}

const fn smart_failure_priority(failure: FailureKind) -> u8 {
    match failure {
        FailureKind::RequiresEscalation => 9,
        FailureKind::PermissionDenied => 8,
        FailureKind::MissingDependency => 7,
        FailureKind::TimedOut => 6,
        FailureKind::ProviderFault => 5,
        FailureKind::TemporarilyUnavailable => 4,
        FailureKind::IdentityChanged => 3,
        FailureKind::Rejected => 2,
        FailureKind::Unsupported => 1,
    }
}

struct NvmeSmartProvider;

impl SmartTelemetryProvider for NvmeSmartProvider {
    fn id(&self) -> ProviderId {
        ProviderId::borrowed("linux.smart.nvme")
    }

    fn supports(&self, request: &SmartDeviceRequest<'_>) -> bool {
        request.connection.device_kind == StorageDeviceKind::Physical
            && request.connection.interconnect != StorageInterconnect::Usb
            && (request.connection.protocol == StorageProtocol::Nvme
                || (request.connection.protocol == StorageProtocol::Unknown
                    && nvme_controller_from_name(request.name).is_some()))
    }

    fn observe(&self, request: &SmartDeviceRequest<'_>) -> DiskSmart {
        read_nvme_smart(request.name)
    }
}

struct AtaSmartProvider;

impl SmartTelemetryProvider for AtaSmartProvider {
    fn id(&self) -> ProviderId {
        ProviderId::borrowed("linux.smart.ata")
    }

    fn supports(&self, request: &SmartDeviceRequest<'_>) -> bool {
        request.connection.device_kind == StorageDeviceKind::Physical
            && request.connection.protocol == StorageProtocol::Ata
            && request.connection.interconnect != StorageInterconnect::Usb
    }

    fn observe(&self, request: &SmartDeviceRequest<'_>) -> DiskSmart {
        read_smartctl_smart(request.name, request.connection)
    }
}

struct ScsiSmartProvider;

impl SmartTelemetryProvider for ScsiSmartProvider {
    fn id(&self) -> ProviderId {
        ProviderId::borrowed("linux.smart.scsi")
    }

    fn supports(&self, request: &SmartDeviceRequest<'_>) -> bool {
        request.connection.device_kind == StorageDeviceKind::Physical
            && request.connection.protocol == StorageProtocol::Scsi
            && request.connection.interconnect != StorageInterconnect::Usb
    }

    fn observe(&self, request: &SmartDeviceRequest<'_>) -> DiskSmart {
        read_smartctl_smart(request.name, request.connection)
    }
}

struct UsbBridgeSmartProvider;

impl SmartTelemetryProvider for UsbBridgeSmartProvider {
    fn id(&self) -> ProviderId {
        ProviderId::borrowed("linux.smart.usb-bridge")
    }

    fn supports(&self, request: &SmartDeviceRequest<'_>) -> bool {
        request.connection.device_kind == StorageDeviceKind::Physical
            && request.connection.interconnect == StorageInterconnect::Usb
    }

    fn observe(&self, request: &SmartDeviceRequest<'_>) -> DiskSmart {
        read_smartctl_smart(request.name, request.connection)
    }
}

struct AutoDetectSmartProvider;

impl SmartTelemetryProvider for AutoDetectSmartProvider {
    fn id(&self) -> ProviderId {
        ProviderId::borrowed("linux.smart.auto-detect")
    }

    fn supports(&self, request: &SmartDeviceRequest<'_>) -> bool {
        request.connection.device_kind == StorageDeviceKind::Physical
            && request.connection.protocol == StorageProtocol::Unknown
            && !matches!(
                request.connection.interconnect,
                StorageInterconnect::Usb
                    | StorageInterconnect::Mmc
                    | StorageInterconnect::Sd
                    | StorageInterconnect::Ufs
                    | StorageInterconnect::Virtio
                    | StorageInterconnect::Network
                    | StorageInterconnect::Platform
            )
    }

    fn observe(&self, request: &SmartDeviceRequest<'_>) -> DiskSmart {
        read_smartctl_smart(request.name, request.connection)
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_smart_provider_tests.rs"]
mod tests;
