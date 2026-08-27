use std::path::{Path, PathBuf};
use std::time::Instant;

use taskmanager_core::{DeviceStatus, GpuMetricField, GpuMetrics, ProviderId};

use super::super::{GpuProviderSample, build_drm_identity_metrics, scan_drm_cards};
use super::{GpuProviderFailure, GpuTelemetryProvider};

pub(super) const DRM_PROVIDER_ID: ProviderId = ProviderId::borrowed("linux.gpu.drm-sysfs");

pub(super) struct DrmSysfsGpuProvider {
    root: PathBuf,
}

impl DrmSysfsGpuProvider {
    pub(super) fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl GpuTelemetryProvider for DrmSysfsGpuProvider {
    fn id(&self) -> ProviderId {
        DRM_PROVIDER_ID
    }

    fn priority(&self) -> u16 {
        10
    }

    fn is_authoritative_inventory(&self) -> bool {
        true
    }

    fn collect(&mut self, _now: Instant) -> Result<Vec<GpuProviderSample>, GpuProviderFailure> {
        verify_directory(&self.root)?;
        Ok(scan_drm_cards(&self.root)
            .into_iter()
            .map(|(card_name, device_path)| {
                let metrics = build_drm_identity_metrics(&card_name, &device_path);
                let fields = fields_for_drm_identity(&metrics);
                GpuProviderSample {
                    metrics,
                    fields,
                    field_failures: Vec::new(),
                }
            })
            .collect())
    }
}

fn fields_for_drm_identity(metric: &GpuMetrics) -> Vec<GpuMetricField> {
    let mut fields = vec![GpuMetricField::Identity, GpuMetricField::Brand];
    if metric.driver.is_some() {
        fields.push(GpuMetricField::Driver);
    }
    fields
}

pub(super) fn verify_directory(path: &Path) -> Result<(), GpuProviderFailure> {
    std::fs::read_dir(path)
        .map(|_| ())
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::PermissionDenied => {
                GpuProviderFailure::new(DeviceStatus::PermissionDenied)
            }
            std::io::ErrorKind::NotFound => GpuProviderFailure::new(DeviceStatus::Unsupported),
            _ => GpuProviderFailure::new(DeviceStatus::Stale),
        })
}
