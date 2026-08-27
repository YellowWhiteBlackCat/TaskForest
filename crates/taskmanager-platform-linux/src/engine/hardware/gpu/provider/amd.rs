use std::path::PathBuf;
use std::time::Instant;

use taskmanager_core::{DeviceStatus, ProviderId};

use super::super::{GpuProviderSample, probe_amdgpu_device, scan_drm_cards};
use super::drm::verify_directory;
use super::{GpuProviderFailure, GpuTelemetryProvider};

pub(super) const AMD_SYSFS_PROVIDER_ID: ProviderId = ProviderId::borrowed("linux.gpu.amdgpu-sysfs");

/// Runtime AMD enrichment. Selection is based on the bound `amdgpu` driver or
/// PCI vendor marker, never a device/SKU list or a build-time product variant.
pub(super) struct AmdSysfsGpuProvider {
    root: PathBuf,
}

impl AmdSysfsGpuProvider {
    pub(super) fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl GpuTelemetryProvider for AmdSysfsGpuProvider {
    fn id(&self) -> ProviderId {
        AMD_SYSFS_PROVIDER_ID
    }

    fn priority(&self) -> u16 {
        50
    }

    fn collect(&mut self, _now: Instant) -> Result<Vec<GpuProviderSample>, GpuProviderFailure> {
        verify_directory(&self.root)?;
        let mut saw_amd = false;
        let samples =
            scan_drm_cards(&self.root)
                .into_iter()
                .filter_map(|(card_name, device_path)| {
                    let probe = probe_amdgpu_device(&card_name, &device_path);
                    saw_amd |= probe.is_amd;
                    probe.sample
                });
        let samples = samples.collect::<Vec<_>>();

        if saw_amd {
            Ok(samples)
        } else {
            Err(GpuProviderFailure::new(DeviceStatus::Unsupported))
        }
    }
}
