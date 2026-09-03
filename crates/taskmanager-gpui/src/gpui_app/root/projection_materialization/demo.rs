//! Demo-only projection seeding for the GPUI materialization boundary.

use super::ProjectionMaterialization;
use taskmanager_shell::SystemProjectionStore;

impl ProjectionMaterialization {
    /// Install a deterministic shell projection for the standalone GPUI demo.
    /// This uses the same revision-keyed materialization path as a platform
    /// batch, but never accepts a platform event or creates a second data
    /// model in the renderer.
    pub(crate) fn seed_from_projection(&mut self, projection: &SystemProjectionStore) {
        let system_revision = projection.system_revision.max(1);
        if let Some(snapshot) = projection.snapshot.clone() {
            self.replace_snapshot(system_revision, snapshot);
        }
        if let Some(processes) = projection.processes.clone() {
            self.replace_processes(
                projection.process_revision.max(1),
                processes,
                projection.processes_observed_at_ms,
            );
        }
        if let Some(services) = projection.services.clone() {
            self.replace_services(
                projection.services_revision.max(1),
                services,
                projection.services_source.clone().unwrap_or_default(),
            );
        }
        if let Some(startup_entries) = projection.startup_entries.clone() {
            self.replace_startup(
                projection.startup_revision.max(1),
                startup_entries,
                projection.startup_source.clone().unwrap_or_default(),
            );
        }
        if let Some(sessions) = projection.sessions.clone() {
            self.replace_sessions(
                projection.sessions_revision.max(1),
                sessions,
                projection.sessions_source.clone().unwrap_or_default(),
            );
        }
        if let Some(hardware) = projection.hardware.clone() {
            self.replace_hardware(
                system_revision,
                hardware,
                projection.hardware_source.clone().unwrap_or_default(),
            );
        }
        if let Some(containers) = projection.containers.clone() {
            self.replace_containers(system_revision, containers);
        }
        if projection.startup_boot_evidence.is_some()
            || projection.startup_evidence_unavailable.is_some()
        {
            self.replace_startup_evidence(
                projection.startup_revision.max(1),
                projection.startup_boot_evidence.clone(),
                projection.startup_evidence_unavailable,
            );
        }
        if let Some(directory_usage) = projection.directory_usage.clone() {
            self.replace_directory_usage(system_revision, Some(directory_usage));
        }
        if let Some(npu_inventory) = projection.npu_inventory.clone() {
            self.replace_npu_inventory(system_revision, Some(npu_inventory));
        }
        self.replace_active_alerts(system_revision, projection.alert_active.clone());
    }
}
