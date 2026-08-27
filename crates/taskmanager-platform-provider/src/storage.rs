use taskmanager_core::{
    DeviceState, DirectoryScanControl, DirectoryScanSpec, DirectoryUsageSnapshot,
    FilesystemHealthSnapshot, SmartSelfTestIntent, SmartSelfTestReport, StorageDeviceTarget,
};
use taskmanager_platform_contract::{CompositeSourceSnapshot, ProviderFailure};

pub trait FilesystemHealthProvider: Send + 'static {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<CompositeSourceSnapshot<FilesystemHealthSnapshot>, ProviderFailure>;
}

pub trait SmartSelfTestControlProvider: Send + 'static {
    /// Start a job from a portable intent that carries stable identity,
    /// lifecycle generation, and an opaque native locator. Display text is not
    /// an address and must never be used to select the target.
    fn start(
        &mut self,
        intent: &SmartSelfTestIntent,
        observed_at_ms: u64,
    ) -> Result<SmartSelfTestReport, ProviderFailure>;
}

pub trait SmartSelfTestObservationProvider: Send + 'static {
    /// Poll a job against the same physical identity and lifecycle generation
    /// selected at control time. Native adapters should re-resolve the opaque
    /// locator and reject identity changes before performing I/O.
    fn refresh(
        &mut self,
        target: &StorageDeviceTarget,
        previous: DeviceState,
        observed_at_ms: u64,
    ) -> Result<SmartSelfTestReport, ProviderFailure>;
}

/// Bounded, cancellable directory-usage scanner.
///
/// The lane drives the scan by calling [`Self::scan_chunk`] repeatedly until
/// the returned snapshot is terminal. Each call performs ONE bounded unit of
/// work (the provider owns its entry/time budget), publishes a bounded
/// Top-N report, and returns a `Scanning` snapshot for more work or a
/// terminal one (`Completed` / `Cancelled` / `Failed`). Providers must never
/// follow symlinks and must poll [`DirectoryScanControl::is_cancelled`]
/// between directories.
pub trait DirectoryUsageProvider: Send + 'static {
    fn scan_chunk(
        &mut self,
        spec: &DirectoryScanSpec,
        control: &DirectoryScanControl,
        observed_at_ms: u64,
    ) -> Result<DirectoryUsageSnapshot, ProviderFailure>;
}
