//! Optional Linux OpenGL/Vulkan runtime capability provider.

use std::path::PathBuf;
use std::time::Instant;

use taskmanager_core::{GpuMetricField, GpuMetrics, ProviderId};

use super::super::{GpuProviderSample, build_drm_identity_metrics, scan_drm_cards};
use super::drm::verify_directory;
use super::{GpuProviderFailure, GpuTelemetryProvider};
use crate::engine::hardware::gpu::api::probe_graphics_api;

pub(super) const GRAPHICS_API_PROVIDER_ID: ProviderId =
    ProviderId::borrowed("linux.gpu.graphics-api.runtime");

pub(super) struct GraphicsApiProvider {
    root: PathBuf,
    probed: bool,
    facts: Option<taskmanager_core::GpuGraphicsApi>,
}

impl GraphicsApiProvider {
    pub(super) fn new(root: PathBuf) -> Self {
        Self {
            root,
            probed: false,
            facts: None,
        }
    }
}

impl GpuTelemetryProvider for GraphicsApiProvider {
    fn id(&self) -> ProviderId {
        GRAPHICS_API_PROVIDER_ID
    }

    fn priority(&self) -> u16 {
        30
    }

    fn collect(&mut self, _now: Instant) -> Result<Vec<GpuProviderSample>, GpuProviderFailure> {
        verify_directory(&self.root)?;
        let cards = scan_drm_cards(&self.root);
        if cards.len() != 1 {
            return Ok(Vec::new());
        }
        if !self.probed {
            self.facts = probe_graphics_api();
            self.probed = true;
        }
        let Some(facts) = self.facts.clone() else {
            return Ok(Vec::new());
        };
        let Some((card_name, device_path)) = cards.into_iter().next() else {
            return Ok(Vec::new());
        };
        let mut metrics: GpuMetrics = build_drm_identity_metrics(&card_name, &device_path);
        metrics.graphics_api = Some(facts);
        Ok(vec![GpuProviderSample {
            metrics,
            fields: vec![GpuMetricField::GraphicsApi],
            field_failures: Vec::new(),
        }])
    }
}
