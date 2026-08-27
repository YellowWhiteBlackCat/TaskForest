//! Linux filesystem-health, SMART self-test, and directory-usage providers
//! bound to shared `StorageExecutors`.
//!
//! Owns `StorageProviders`, which adapts the four storage registrations into
//! `StorageProviderBindings`. Directory-usage scans are pure safe `std::fs`
//! (route-C, ADR-019): no escalation, cancellable, symlink-safe.

use taskmanager_application::{
    DirectoryUsageRequest, SmartControlRequest, SmartObservationRequest, StorageHealthRequest,
};
use taskmanager_platform_provider::{
    DirectoryUsageProvider, FilesystemHealthProvider, SmartSelfTestControlProvider,
    SmartSelfTestObservationProvider,
};
use taskmanager_platform_runtime::{
    ProviderRegistration, StorageExecutors, StorageProviderBindings,
};

type FilesystemHealthRegistration =
    ProviderRegistration<StorageHealthRequest, Box<dyn FilesystemHealthProvider>>;
type SmartObservationRegistration =
    ProviderRegistration<SmartObservationRequest, Box<dyn SmartSelfTestObservationProvider>>;
type SmartControlRegistration =
    ProviderRegistration<SmartControlRequest, Box<dyn SmartSelfTestControlProvider>>;
type DirectoryUsageRegistration =
    ProviderRegistration<DirectoryUsageRequest, Box<dyn DirectoryUsageProvider>>;

/// Linux filesystem-health, SMART, and directory-usage providers adapted to
/// shared executors.
pub struct StorageProviders {
    filesystems: FilesystemHealthRegistration,
    smart_observation: SmartObservationRegistration,
    smart_control: SmartControlRegistration,
    directory_usage: DirectoryUsageRegistration,
}

impl StorageProviders {
    #[must_use]
    pub fn new<H, O, C, D>(
        filesystems: ProviderRegistration<StorageHealthRequest, H>,
        smart_observation: ProviderRegistration<SmartObservationRequest, O>,
        smart_control: ProviderRegistration<SmartControlRequest, C>,
        directory_usage: ProviderRegistration<DirectoryUsageRequest, D>,
    ) -> Self
    where
        H: FilesystemHealthProvider,
        O: SmartSelfTestObservationProvider,
        C: SmartSelfTestControlProvider,
        D: DirectoryUsageProvider,
    {
        Self {
            filesystems: filesystems
                .map_provider(|provider| Box::new(provider) as Box<dyn FilesystemHealthProvider>),
            smart_observation: smart_observation.map_provider(|provider| {
                Box::new(provider) as Box<dyn SmartSelfTestObservationProvider>
            }),
            smart_control: smart_control.map_provider(|provider| {
                Box::new(provider) as Box<dyn SmartSelfTestControlProvider>
            }),
            directory_usage: directory_usage
                .map_provider(|provider| Box::new(provider) as Box<dyn DirectoryUsageProvider>),
        }
    }

    pub(crate) fn runtime_bindings(&self) -> StorageProviderBindings {
        StorageProviderBindings::from_registrations(
            &self.filesystems,
            &self.smart_observation,
            &self.smart_control,
        )
        .with_directory_usage(&self.directory_usage)
    }

    pub(crate) fn into_runtime(self) -> StorageExecutors {
        let Self {
            filesystems,
            smart_observation,
            smart_control,
            directory_usage,
        } = self;
        let mut filesystems = filesystems.into_provider();
        let mut smart_observation = smart_observation.into_provider();
        let mut smart_control = smart_control.into_provider();
        let mut directory_usage = directory_usage.into_provider();
        StorageExecutors::new(
            move |observed_at_ms| filesystems.refresh(observed_at_ms),
            move |target, previous, observed_at_ms| {
                smart_observation.refresh(target, previous, observed_at_ms)
            },
            move |intent, observed_at_ms| smart_control.start(intent, observed_at_ms),
        )
        .with_directory_usage(move |spec, control, observed_at_ms| {
            directory_usage.scan_chunk(spec, control, observed_at_ms)
        })
    }
}
