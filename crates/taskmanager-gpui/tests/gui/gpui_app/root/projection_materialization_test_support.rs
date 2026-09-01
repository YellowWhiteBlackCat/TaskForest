use super::super::RootView;
use crate::gpui_app::process_insights::ProcessInsightsState;
use taskmanager_application::StartupEvidenceUnavailable;
use taskmanager_core::core::Alert;
use taskmanager_core::core::directory_usage::DirectoryUsageSnapshot;
use taskmanager_core::core::hardware::HardwareInfo;
use taskmanager_core::core::metrics::SystemSnapshot;
use taskmanager_core::core::npu::NpuInventorySnapshot;
use taskmanager_core::core::power::PowerSupplySnapshot;
use taskmanager_core::core::process::ProcessItem;
use taskmanager_core::core::process::ProcessLiveKey;
use taskmanager_core::core::process_telemetry::ContainerRollup;
use taskmanager_core::core::sensors::SensorCenterSnapshot;
use taskmanager_core::core::services::ServiceItem;
use taskmanager_core::core::session::SessionItem;
use taskmanager_core::core::source::SourceStatus;
use taskmanager_core::core::startup::StartupBootEvidenceSnapshot;
use taskmanager_core::core::startup::StartupEntry;
use taskmanager_core::core::storage_health::FilesystemHealthSnapshot;

impl RootView {
    pub fn replace_process_insights_for_test(&mut self, state: ProcessInsightsState) {
        let identity = match &state {
            ProcessInsightsState::Loading { identity } => Some(*identity),
            ProcessInsightsState::Ready(snapshot) => {
                ProcessLiveKey::from_identity(snapshot.identity)
            }
            ProcessInsightsState::Error(error) => error.identity,
        };
        if let Some(identity) = identity
            && let Some(target) = self.frozen_process(identity)
        {
            self.process_insights.install_capture_state(target, state);
        }
    }

    pub fn replace_containers_for_test(&mut self, containers: ContainerRollup) {
        let revision = self.materialized.containers.revision.saturating_add(1);
        self.materialized.replace_containers(revision, containers);
    }

    pub fn replace_startup_evidence_for_test(
        &mut self,
        evidence: Option<StartupBootEvidenceSnapshot>,
        unavailable: Option<StartupEvidenceUnavailable>,
    ) {
        let revision = self
            .materialized
            .startup_evidence
            .revision
            .saturating_add(1);
        self.materialized
            .replace_startup_evidence(revision, evidence, unavailable);
    }

    pub fn replace_directory_usage_for_test(&mut self, snapshot: Option<DirectoryUsageSnapshot>) {
        let revision = self.materialized.directory_usage.revision.saturating_add(1);
        self.materialized
            .replace_directory_usage(revision, snapshot);
    }

    pub fn replace_npu_inventory_for_test(&mut self, snapshot: Option<NpuInventorySnapshot>) {
        let revision = self.materialized.npu_inventory.revision.saturating_add(1);
        self.materialized.replace_npu_inventory(revision, snapshot);
    }

    pub fn replace_dynamic_devices_for_test(
        &mut self,
        sensors: SensorCenterSnapshot,
        power_supplies: PowerSupplySnapshot,
    ) {
        let revision = self
            .materialized
            .sensors
            .materialized
            .revision
            .max(self.materialized.power_supplies.materialized.revision)
            .saturating_add(1);
        let _ = self
            .materialized
            .replace_sensors(revision, sensors, Vec::new());
        let _ = self
            .materialized
            .replace_power_supplies(revision, power_supplies, Vec::new());
    }

    pub fn replace_storage_health_for_test(&mut self, filesystems: FilesystemHealthSnapshot) {
        let revision = self
            .materialized
            .storage_health
            .materialized
            .revision
            .saturating_add(1);
        self.materialized
            .replace_storage_health(revision, filesystems, Vec::new());
    }

    pub fn replace_active_alerts_for_test(&mut self, alerts: Vec<Alert>) {
        let revision = self.materialized.active_alerts.revision.saturating_add(1);
        self.materialized.replace_active_alerts(revision, alerts);
    }

    pub fn process_insights_is_idle_for_test(&self) -> bool {
        matches!(
            self.process_insights,
            super::super::process_insights_ui::ProcessInsightsLifecycle::Idle
        )
    }

    pub fn replace_system_snapshot_for_test(&mut self, snapshot: SystemSnapshot) {
        let revision = self.system_snapshot_generation().saturating_add(1);
        self.materialized.replace_snapshot(revision, snapshot);
    }

    pub fn system_snapshot_mut_for_test(&mut self) -> &mut SystemSnapshot {
        std::rc::Rc::make_mut(&mut self.materialized.snapshot.value)
    }

    pub fn replace_processes_for_test(&mut self, processes: Vec<ProcessItem>) {
        let revision = self.processes_generation().saturating_add(1);
        taskmanager_shell::fixture::seed_direct_track_fact(
            &mut self.shell,
            taskmanager_shell::fixture::DirectTrackSeedFact::Processes(processes.clone()),
        );
        self.materialized.replace_processes(
            revision,
            std::sync::Arc::new(processes),
            self.processes_observed_at_ms(),
        );
    }

    pub fn processes_mut_for_test(&mut self) -> &mut Vec<ProcessItem> {
        std::sync::Arc::make_mut(&mut self.materialized.processes.value)
    }

    pub fn hardware_mut_for_test(&mut self) -> &mut HardwareInfo {
        std::rc::Rc::make_mut(&mut self.materialized.hardware.materialized.value)
    }

    pub fn replace_services_for_test(
        &mut self,
        services: Vec<ServiceItem>,
        sources: Vec<SourceStatus>,
    ) {
        let revision = self.services_generation().saturating_add(1);
        self.materialized
            .replace_services(revision, services, sources);
    }

    pub fn advance_services_generation_for_test(&mut self) {
        let services = self.services().to_vec();
        let sources = self.service_sources().to_vec();
        self.replace_services_for_test(services, sources);
    }

    pub fn replace_startup_for_test(
        &mut self,
        entries: Vec<StartupEntry>,
        sources: Vec<SourceStatus>,
    ) {
        let revision = self.startup_generation().saturating_add(1);
        self.materialized
            .replace_startup(revision, entries, sources);
    }

    pub fn advance_startup_generation_for_test(&mut self) {
        let entries = self.startup_entries().to_vec();
        let sources = self.startup_sources().to_vec();
        self.replace_startup_for_test(entries, sources);
    }

    pub fn replace_sessions_for_test(
        &mut self,
        sessions: Vec<SessionItem>,
        sources: Vec<SourceStatus>,
    ) {
        let revision = self.sessions_generation().saturating_add(1);
        self.materialized
            .replace_sessions(revision, sessions, sources);
    }

    pub fn advance_sessions_generation_for_test(&mut self) {
        let sessions = self.sessions().to_vec();
        let sources = self.session_sources().to_vec();
        self.replace_sessions_for_test(sessions, sources);
    }

    pub fn replace_hardware_for_test(
        &mut self,
        hardware: HardwareInfo,
        sources: Vec<SourceStatus>,
    ) {
        let revision = self.hardware_generation().saturating_add(1);
        self.materialized
            .replace_hardware(revision, hardware, sources);
    }
}
