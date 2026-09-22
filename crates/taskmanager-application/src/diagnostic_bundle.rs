//! Privacy-safe neutral diagnostic bundle export engine and application port.
//!
//! Collects structured diagnostic facts across system overview, platform capabilities,
//! configuration, process summary/counts, and telemetry health.
//! Sanitization of usernames, paths, and IP addresses delegates to the audited
//! core diagnostics contract (`DiagnosticBundlePlan::prepare`).

use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use atomicwrites::{AllowOverwrite, AtomicFile};
use serde::{Deserialize, Serialize};

use taskmanager_core::config::Config;
use taskmanager_core::core::hardware::HardwareInfo;
use taskmanager_core::core::metrics::SystemSnapshot;
use taskmanager_core::core::process::ProcessItem;
use taskmanager_core::{
    DiagnosticBundleError, DiagnosticBundleErrorKind, DiagnosticBundlePlan, DiagnosticSource,
};
use taskmanager_platform_contract::CapabilityCatalog;

use crate::PlatformClient;
use crate::RefreshRequest;
use crate::platform::{HardwareInventoryEvent, ProcessEvent, ProjectedSystemTelemetry};

pub const DIAGNOSTIC_BUNDLE_SCHEMA_VERSION: u32 = 1;

#[path = "diagnostic_bundle/models.rs"]
mod models;
#[path = "diagnostic_bundle/report.rs"]
mod report;
pub use models::*;
pub use report::*;

/// Neutral diagnostic bundle containing all structured diagnostic facts.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DiagnosticBundle {
    pub version: u32,
    pub timestamp_ms: u64,
    pub system_overview: DiagnosticSystemOverview,
    pub platform_capabilities: DiagnosticPlatformCapabilities,
    pub configuration: serde_json::Value,
    pub process_summary: DiagnosticProcessSummary,
    pub telemetry_health: DiagnosticTelemetryHealth,
}

impl DiagnosticBundle {
    #[must_use]
    pub fn build(
        hardware: Option<&HardwareInfo>,
        snapshot: Option<&SystemSnapshot>,
        projection: Option<&ProjectedSystemTelemetry>,
        catalog: Option<&dyn CapabilityCatalog>,
        config: Option<&Config>,
        processes: &[ProcessItem],
    ) -> Self {
        let timestamp_ms = snapshot
            .map(|s| s.timestamp_ms)
            .filter(|ts| *ts > 0)
            .unwrap_or_else(current_time_ms);

        let system_overview = DiagnosticSystemOverview::from_facts(hardware, snapshot);
        let platform_capabilities = catalog
            .map(DiagnosticPlatformCapabilities::from_catalog)
            .unwrap_or_default();
        let configuration = config
            .and_then(|c| serde_json::to_value(c).ok())
            .unwrap_or_else(|| serde_json::to_value(Config::default()).unwrap_or_default());
        let threads = snapshot.and_then(|s| s.threads);
        let process_summary = DiagnosticProcessSummary::from_processes(processes, threads);
        let telemetry_health = DiagnosticTelemetryHealth::from_telemetry(projection, snapshot);

        Self {
            version: DIAGNOSTIC_BUNDLE_SCHEMA_VERSION,
            timestamp_ms,
            system_overview,
            platform_capabilities,
            configuration,
            process_summary,
            telemetry_health,
        }
    }

    #[must_use]
    pub fn manifest(&self) -> DiagnosticBundleManifest {
        DiagnosticBundleManifest {
            version: self.version,
            timestamp_ms: self.timestamp_ms,
            sources: vec![
                "manifest.json".to_owned(),
                "system-overview.json".to_owned(),
                "platform-capabilities.json".to_owned(),
                "configuration.json".to_owned(),
                "process-summary.json".to_owned(),
                "telemetry-health.json".to_owned(),
            ],
            total_processes: self.process_summary.total_processes,
            capabilities_count: self.platform_capabilities.capabilities.len(),
        }
    }

    /// Collect observed usernames to seed the core redaction contract.
    #[must_use]
    pub fn collected_usernames(&self) -> Vec<String> {
        let mut usernames = HashSet::new();
        for top in &self.process_summary.top_cpu {
            if let Some(user) = &top.user
                && !user.trim().is_empty()
            {
                usernames.insert(user.clone());
            }
        }
        for top in &self.process_summary.top_memory {
            if let Some(user) = &top.user
                && !user.trim().is_empty()
            {
                usernames.insert(user.clone());
            }
        }
        if let Ok(user) = std::env::var("USER")
            && !user.trim().is_empty()
        {
            usernames.insert(user);
        }
        if let Ok(user) = std::env::var("USERNAME")
            && !user.trim().is_empty()
        {
            usernames.insert(user);
        }
        usernames.into_iter().collect()
    }

    /// Package the structured diagnostic facts into an immutable, sanitized
    /// [`DiagnosticBundlePlan`] where usernames, paths, and IP addresses are
    /// redacted by core.
    pub fn to_plan(
        &self,
        extra_usernames: impl IntoIterator<Item = String>,
    ) -> Result<DiagnosticBundlePlan, DiagnosticBundleError> {
        let mut all_usernames = self.collected_usernames();
        all_usernames.extend(extra_usernames);

        let manifest_json = serde_json::to_string_pretty(&self.manifest()).map_err(encode_error)?;
        let overview_json =
            serde_json::to_string_pretty(&self.system_overview).map_err(encode_error)?;
        let capabilities_json =
            serde_json::to_string_pretty(&self.platform_capabilities).map_err(encode_error)?;
        let config_json =
            serde_json::to_string_pretty(&self.configuration).map_err(encode_error)?;
        let process_json =
            serde_json::to_string_pretty(&self.process_summary).map_err(encode_error)?;
        let health_json =
            serde_json::to_string_pretty(&self.telemetry_health).map_err(encode_error)?;
        let bundle_json = serde_json::to_string_pretty(self).map_err(encode_error)?;

        let sources = vec![
            DiagnosticSource {
                name: "manifest.json".to_owned(),
                contents: manifest_json,
            },
            DiagnosticSource {
                name: "system-overview.json".to_owned(),
                contents: overview_json,
            },
            DiagnosticSource {
                name: "platform-capabilities.json".to_owned(),
                contents: capabilities_json,
            },
            DiagnosticSource {
                name: "configuration.json".to_owned(),
                contents: config_json,
            },
            DiagnosticSource {
                name: "process-summary.json".to_owned(),
                contents: process_json,
            },
            DiagnosticSource {
                name: "telemetry-health.json".to_owned(),
                contents: health_json,
            },
            DiagnosticSource {
                name: "diagnostic-bundle.json".to_owned(),
                contents: bundle_json,
            },
        ];

        DiagnosticBundlePlan::prepare(sources, all_usernames)
    }

    /// Serialize the bundle directly to pretty JSON.
    pub fn to_json_string(&self) -> Result<String, DiagnosticBundleError> {
        serde_json::to_string_pretty(self).map_err(encode_error)
    }

    /// Export the sanitized bundle plan to `path` using transactional atomic writes.
    pub fn export_to_file(
        &self,
        path: &Path,
        extra_usernames: impl IntoIterator<Item = String>,
    ) -> Result<PathBuf, DiagnosticBundleError> {
        if path.as_os_str().is_empty() {
            return Err(DiagnosticBundleError::with_detail(
                DiagnosticBundleErrorKind::InvalidTarget,
                "diagnostic bundle export destination path is empty",
            ));
        }

        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent).map_err(|err| {
                DiagnosticBundleError::with_detail(
                    DiagnosticBundleErrorKind::Io,
                    format!(
                        "failed to create parent directory {}: {err}",
                        parent.display()
                    ),
                )
            })?;
        }

        let plan = self.to_plan(extra_usernames)?;
        let bytes = plan.encoded()?;

        AtomicFile::new(path, AllowOverwrite)
            .write(|file| {
                file.write_all(&bytes)?;
                file.sync_all()
            })
            .map_err(|err| {
                DiagnosticBundleError::with_detail(
                    DiagnosticBundleErrorKind::Io,
                    format!(
                        "failed to write diagnostic bundle to {}: {err}",
                        path.display()
                    ),
                )
            })?;

        Ok(path.to_path_buf())
    }

    /// Application-level method to export the diagnostic bundle to `path`.
    pub fn export_diagnostic_bundle(&self, path: &Path) -> Result<PathBuf, DiagnosticBundleError> {
        self.export_to_file(path, [])
    }

    /// Generate a human-readable Markdown diagnostic report.
    /// Generate a human-readable Markdown diagnostic report.
    #[must_use]
    pub fn generate_diagnostic_report(&self) -> String {
        report::format_markdown_report(self)
    }

    /// Generate a redacted diagnostic report using the core redaction contract.
    pub fn generate_redacted_report(
        &self,
        extra_usernames: impl IntoIterator<Item = String>,
    ) -> Result<String, DiagnosticBundleError> {
        let unredacted = self.generate_diagnostic_report();
        let mut all_usernames = self.collected_usernames();
        all_usernames.extend(extra_usernames);

        let plan = DiagnosticBundlePlan::prepare(
            vec![DiagnosticSource {
                name: "diagnostic-report.md".to_owned(),
                contents: unredacted,
            }],
            all_usernames,
        )?;

        plan.sanitized_contents("diagnostic-report.md")
            .map(ToOwned::to_owned)
            .ok_or_else(|| {
                DiagnosticBundleError::with_detail(
                    DiagnosticBundleErrorKind::Encode,
                    "sanitized diagnostic report missing from plan",
                )
            })
    }
}

fn encode_error(err: serde_json::Error) -> DiagnosticBundleError {
    DiagnosticBundleError::with_detail(DiagnosticBundleErrorKind::Encode, err.to_string())
}

/// Neutral diagnostic bundle export engine service.
#[derive(Debug, Clone, Copy, Default)]
pub struct DiagnosticBundleEngine;

impl DiagnosticBundleEngine {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    #[must_use]
    pub fn build(
        &self,
        hardware: Option<&HardwareInfo>,
        snapshot: Option<&SystemSnapshot>,
        projection: Option<&ProjectedSystemTelemetry>,
        catalog: Option<&dyn CapabilityCatalog>,
        config: Option<&Config>,
        processes: &[ProcessItem],
    ) -> DiagnosticBundle {
        DiagnosticBundle::build(hardware, snapshot, projection, catalog, config, processes)
    }

    pub fn export_bundle(
        &self,
        bundle: &DiagnosticBundle,
        path: &Path,
    ) -> Result<PathBuf, DiagnosticBundleError> {
        bundle.export_diagnostic_bundle(path)
    }

    #[must_use]
    pub fn generate_report(&self, bundle: &DiagnosticBundle) -> String {
        bundle.generate_diagnostic_report()
    }
}

/// Neutral port trait for diagnostic bundle export operations.
pub trait DiagnosticBundleExportPort {
    fn export_diagnostic_bundle(&mut self, path: &Path) -> Result<PathBuf, DiagnosticBundleError>;
    fn generate_diagnostic_report(&mut self) -> Result<String, DiagnosticBundleError>;
}

impl DiagnosticBundleExportPort for PlatformClient {
    fn export_diagnostic_bundle(&mut self, path: &Path) -> Result<PathBuf, DiagnosticBundleError> {
        PlatformClient::export_diagnostic_bundle(self, path)
    }

    fn generate_diagnostic_report(&mut self) -> Result<String, DiagnosticBundleError> {
        PlatformClient::generate_diagnostic_report(self)
    }
}

impl PlatformClient {
    /// Collect the structured diagnostic bundle from the live platform client.
    pub fn collect_diagnostic_bundle(
        &mut self,
        config: Option<&Config>,
        timeout: Duration,
    ) -> Result<DiagnosticBundle, DiagnosticBundleError> {
        collect_diagnostic_bundle_from_client(self, config, timeout)
    }

    /// Export the structured diagnostic bundle to a destination path.
    pub fn export_diagnostic_bundle(
        &mut self,
        path: &Path,
    ) -> Result<PathBuf, DiagnosticBundleError> {
        let bundle = self.collect_diagnostic_bundle(None, Duration::from_secs(5))?;
        export_diagnostic_bundle(&bundle, path)
    }

    /// Generate a human-readable diagnostic report string from live state.
    pub fn generate_diagnostic_report(&mut self) -> Result<String, DiagnosticBundleError> {
        let bundle = self.collect_diagnostic_bundle(None, Duration::from_secs(5))?;
        Ok(bundle.generate_diagnostic_report())
    }
}

/// Application-level method to export a diagnostic bundle to a path.
pub fn export_diagnostic_bundle(
    bundle: &DiagnosticBundle,
    path: &Path,
) -> Result<PathBuf, DiagnosticBundleError> {
    bundle.export_diagnostic_bundle(path)
}

/// Application-level method to generate a diagnostic report string.
#[must_use]
pub fn generate_diagnostic_report(bundle: &DiagnosticBundle) -> String {
    bundle.generate_diagnostic_report()
}

/// Collect a point-in-time diagnostic bundle from an active [`PlatformClient`].
pub fn collect_diagnostic_bundle_from_client(
    client: &mut PlatformClient,
    config: Option<&Config>,
    timeout: Duration,
) -> Result<DiagnosticBundle, DiagnosticBundleError> {
    let warmup = Duration::from_millis(150);
    std::thread::sleep(warmup);

    let submitted_at_ms = current_time_ms();
    let _ = client.request_refresh(RefreshRequest::Telemetry, submitted_at_ms);
    let _ = client.request_refresh(RefreshRequest::Processes, submitted_at_ms);
    let _ = client.request_refresh(RefreshRequest::HardwareInventory, submitted_at_ms);

    let started = std::time::Instant::now();
    let mut snapshot: Option<SystemSnapshot> = None;
    let mut processes: Vec<ProcessItem> = Vec::new();
    let mut hardware: Option<HardwareInfo> = None;
    let mut have_processes = false;

    let poll_interval = Duration::from_millis(20);
    loop {
        if let Ok(batch) = client.try_drain() {
            for projection in batch.system_telemetry_projections {
                if let Some(complete) = projection
                    .complete_snapshot()
                    .or_else(|| projection.render_snapshot())
                {
                    snapshot = Some(complete);
                }
            }

            for correlated in batch.process_events {
                if let ProcessEvent::Snapshot(items) = correlated.event {
                    processes = items.as_ref().clone();
                    have_processes = true;
                }
            }

            for correlated in batch.hardware_inventory_events {
                let HardwareInventoryEvent::Snapshot(snap) = correlated.event;
                hardware = Some(snap.value);
            }
        }

        if (snapshot.is_some() && have_processes) || started.elapsed() >= timeout {
            break;
        }
        std::thread::sleep(poll_interval);
    }

    let snapshot = snapshot.ok_or_else(|| {
        DiagnosticBundleError::with_detail(
            DiagnosticBundleErrorKind::Unavailable,
            format!(
                "no complete telemetry snapshot arrived within {} ms",
                timeout.as_millis()
            ),
        )
    })?;

    let catalog = client.capabilities();
    let projection = client.system_telemetry_projection();

    Ok(DiagnosticBundle::build(
        hardware.as_ref(),
        Some(&snapshot),
        projection.as_ref(),
        Some(catalog),
        config,
        &processes,
    ))
}

/// Collect a point-in-time diagnostic bundle from an active client and export to `path`.
pub fn export_client_diagnostic_bundle(
    client: &mut PlatformClient,
    config: Option<&Config>,
    path: &Path,
    timeout: Duration,
) -> Result<PathBuf, DiagnosticBundleError> {
    let bundle = collect_diagnostic_bundle_from_client(client, config, timeout)?;
    export_diagnostic_bundle(&bundle, path)
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "../tests/headless/application_diagnostic_bundle_tests.rs"]
mod tests;
