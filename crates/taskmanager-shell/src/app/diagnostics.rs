//! Frozen diagnostic plans prepared from observed, cached facts.

use taskmanager_application::DiagnosticBundleEngine;
use taskmanager_core::core::config::Config;
use taskmanager_core::core::diagnostics::{
    DiagnosticBundleError, DiagnosticBundleErrorKind, DiagnosticBundlePlan,
};

use super::SystemProjectionStore;

impl SystemProjectionStore {
    /// A missing snapshot or process inventory is unavailable, never an
    /// empty success-looking report. Redaction runs before any preview exists.
    pub fn prepare_diagnostic_bundle(
        &self,
        config: Option<&Config>,
    ) -> Result<DiagnosticBundlePlan, DiagnosticBundleError> {
        let (Some(snapshot), Some(processes)) = (&self.snapshot, &self.processes) else {
            return Err(DiagnosticBundleError::new(
                DiagnosticBundleErrorKind::Unavailable,
            ));
        };
        let bundle = DiagnosticBundleEngine::new().build(
            self.hardware.as_ref(),
            Some(snapshot),
            self.system_telemetry.as_ref(),
            Some(&self.capabilities),
            config,
            processes,
        );
        bundle.prepare_review_plan(
            snapshot,
            processes,
            self.services.as_deref(),
            self.startup_entries.as_deref(),
        )
    }
}
