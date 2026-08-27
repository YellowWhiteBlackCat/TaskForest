use std::path::PathBuf;
use std::time::Instant;

use taskmanager_core::ProviderId;

use super::super::GpuProviderSample;
#[cfg(feature = "nvidia")]
use super::super::collect_nvidia_nvml;
use super::super::nvidia::collect_nvidia_procfs_samples;
use super::{GpuProviderFailure, GpuTelemetryProvider};

pub(super) const NVIDIA_PROCFS_PROVIDER_ID: ProviderId =
    ProviderId::borrowed("linux.gpu.nvidia-procfs");
#[cfg(feature = "nvidia")]
pub(super) const NVML_PROVIDER_ID: ProviderId = ProviderId::borrowed("linux.gpu.nvml");

pub(super) struct NvidiaProcfsGpuProvider {
    root: PathBuf,
}

impl NvidiaProcfsGpuProvider {
    pub(super) fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl GpuTelemetryProvider for NvidiaProcfsGpuProvider {
    fn id(&self) -> ProviderId {
        NVIDIA_PROCFS_PROVIDER_ID
    }

    fn priority(&self) -> u16 {
        20
    }

    fn collect(&mut self, _now: Instant) -> Result<Vec<GpuProviderSample>, GpuProviderFailure> {
        collect_nvidia_procfs_samples(&self.root).map_err(GpuProviderFailure::new)
    }
}

#[cfg(feature = "nvidia")]
pub(super) struct NvmlGpuProvider;

#[cfg(feature = "nvidia")]
impl GpuTelemetryProvider for NvmlGpuProvider {
    fn id(&self) -> ProviderId {
        NVML_PROVIDER_ID
    }

    fn priority(&self) -> u16 {
        100
    }

    fn collect(&mut self, _now: Instant) -> Result<Vec<GpuProviderSample>, GpuProviderFailure> {
        collect_nvidia_nvml().map_err(GpuProviderFailure::new)
    }
}
