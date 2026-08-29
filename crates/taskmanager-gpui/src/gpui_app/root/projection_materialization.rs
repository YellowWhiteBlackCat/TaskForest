//! Revision-keyed GPUI read models materialized from `SystemProjectionStore`.
//!
//! The shared shell projection remains data truth. This component owns the
//! window-local `Rc`/memo-friendly representation and its exact invalidation
//! keys; only platform-batch materialization systems may replace production
//! values. Render and input code consume immutable `RootView` accessors.

use std::{rc::Rc, sync::Arc};

use taskmanager_application::StartupEvidenceUnavailable;
use taskmanager_core::core::Alert;
use taskmanager_core::core::directory_usage::DirectoryUsageSnapshot;
use taskmanager_core::core::hardware::HardwareInfo;
use taskmanager_core::core::metrics::SystemSnapshot;
use taskmanager_core::core::npu::NpuInventorySnapshot;
use taskmanager_core::core::power::PowerSupplySnapshot;
use taskmanager_core::core::process::ProcessItem;
use taskmanager_core::core::process_telemetry::ContainerRollup;
use taskmanager_core::core::sensors::SensorCenterSnapshot;
use taskmanager_core::core::services::ServiceItem;
use taskmanager_core::core::session::SessionItem;
use taskmanager_core::core::source::SourceStatus;
use taskmanager_core::core::startup::{StartupBootEvidenceSnapshot, StartupEntry};

#[derive(Clone, Debug, Default, PartialEq)]
struct StartupEvidenceProjection {
    snapshot: Option<StartupBootEvidenceSnapshot>,
    unavailable: Option<StartupEvidenceUnavailable>,
}

#[derive(Clone, Debug)]
struct Materialized<T> {
    /// Exact shared-projection generation that produced `value`. Equal or
    /// stale revisions are rejected, so unchanged domains retain `Rc` identity
    /// and revision saturation fails closed on the last coherent value.
    revision: u64,
    value: T,
}

impl<T: Default> Default for Materialized<T> {
    fn default() -> Self {
        Self {
            revision: 0,
            value: T::default(),
        }
    }
}

impl<T> Materialized<T> {
    fn replace(&mut self, revision: u64, value: T) -> bool {
        if revision <= self.revision {
            return false;
        }
        self.revision = revision;
        self.value = value;
        true
    }
}

#[derive(Clone, Debug)]
struct SourcedMaterialized<T> {
    /// The value and sources are one domain snapshot. Only `replace` may
    /// advance them, preventing rows from being observed with another
    /// generation's provider status.
    materialized: Materialized<T>,
    sources: Rc<Vec<SourceStatus>>,
}

impl<T: Default> Default for SourcedMaterialized<T> {
    fn default() -> Self {
        Self {
            materialized: Materialized::default(),
            sources: Rc::new(Vec::new()),
        }
    }
}

impl<T> SourcedMaterialized<T> {
    fn replace(&mut self, revision: u64, value: T, sources: Vec<SourceStatus>) -> bool {
        if !self.materialized.replace(revision, value) {
            return false;
        }
        self.sources = Rc::new(sources);
        true
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct ProjectionMaterialization {
    snapshot: Materialized<Rc<SystemSnapshot>>,
    /// Production process events carry an `Arc<Vec<ProcessItem>>`; retaining
    /// that same allocation here avoids a second full row snapshot in every
    /// GPUI window. The shell projection remains the shared authority consumed
    /// by the other frontends.
    processes: Materialized<Arc<Vec<ProcessItem>>>,
    running_process_count: usize,
    services: SourcedMaterialized<Rc<Vec<ServiceItem>>>,
    startup_entries: SourcedMaterialized<Rc<Vec<StartupEntry>>>,
    sessions: SourcedMaterialized<Rc<Vec<SessionItem>>>,
    hardware: SourcedMaterialized<Rc<HardwareInfo>>,
    containers: Materialized<Rc<ContainerRollup>>,
    startup_evidence: Materialized<Rc<StartupEvidenceProjection>>,
    sensors: SourcedMaterialized<Rc<SensorCenterSnapshot>>,
    power_supplies: SourcedMaterialized<Rc<PowerSupplySnapshot>>,
    directory_usage: Materialized<Option<Rc<DirectoryUsageSnapshot>>>,
    npu_inventory: Materialized<Option<Rc<NpuInventorySnapshot>>>,
    active_alerts: Materialized<Rc<Vec<Alert>>>,
    storage_health:
        SourcedMaterialized<Rc<taskmanager_core::core::storage_health::FilesystemHealthSnapshot>>,
}

impl ProjectionMaterialization {
    pub(super) fn replace_snapshot(&mut self, revision: u64, snapshot: SystemSnapshot) {
        let _ = self.snapshot.replace(revision, Rc::new(snapshot));
    }

    pub(super) fn replace_processes(&mut self, revision: u64, processes: Arc<Vec<ProcessItem>>) {
        let running_process_count = processes
            .iter()
            .filter(|process| process.status == "Running")
            .count();
        if self.processes.replace(revision, processes) {
            self.running_process_count = running_process_count;
        }
    }

    pub(super) fn replace_services(
        &mut self,
        revision: u64,
        services: Vec<ServiceItem>,
        sources: Vec<SourceStatus>,
    ) {
        let _ = self.services.replace(revision, Rc::new(services), sources);
    }

    pub(super) fn replace_startup(
        &mut self,
        revision: u64,
        entries: Vec<StartupEntry>,
        sources: Vec<SourceStatus>,
    ) {
        let _ = self
            .startup_entries
            .replace(revision, Rc::new(entries), sources);
    }

    pub(super) fn replace_sessions(
        &mut self,
        revision: u64,
        sessions: Vec<SessionItem>,
        sources: Vec<SourceStatus>,
    ) {
        let _ = self.sessions.replace(revision, Rc::new(sessions), sources);
    }

    pub(super) fn replace_hardware(
        &mut self,
        revision: u64,
        hardware: HardwareInfo,
        sources: Vec<SourceStatus>,
    ) {
        let _ = self.hardware.replace(revision, Rc::new(hardware), sources);
    }

    pub(super) fn replace_containers(&mut self, revision: u64, containers: ContainerRollup) {
        let _ = self.containers.replace(revision, Rc::new(containers));
    }

    pub(super) fn replace_startup_evidence(
        &mut self,
        revision: u64,
        snapshot: Option<StartupBootEvidenceSnapshot>,
        unavailable: Option<StartupEvidenceUnavailable>,
    ) {
        let _ = self.startup_evidence.replace(
            revision,
            Rc::new(StartupEvidenceProjection {
                snapshot,
                unavailable,
            }),
        );
    }

    pub(super) fn replace_sensors(
        &mut self,
        revision: u64,
        sensors: SensorCenterSnapshot,
        sources: Vec<SourceStatus>,
    ) -> bool {
        self.sensors.replace(revision, Rc::new(sensors), sources)
    }

    pub(super) fn replace_power_supplies(
        &mut self,
        revision: u64,
        power_supplies: PowerSupplySnapshot,
        sources: Vec<SourceStatus>,
    ) -> bool {
        self.power_supplies
            .replace(revision, Rc::new(power_supplies), sources)
    }

    pub(super) fn replace_directory_usage(
        &mut self,
        revision: u64,
        snapshot: Option<DirectoryUsageSnapshot>,
    ) {
        let _ = self
            .directory_usage
            .replace(revision, snapshot.map(Rc::new));
    }

    pub(super) fn replace_npu_inventory(
        &mut self,
        revision: u64,
        snapshot: Option<NpuInventorySnapshot>,
    ) {
        let _ = self.npu_inventory.replace(revision, snapshot.map(Rc::new));
    }

    pub(super) fn replace_active_alerts(&mut self, revision: u64, alerts: Vec<Alert>) {
        let _ = self.active_alerts.replace(revision, Rc::new(alerts));
    }

    pub(super) fn replace_storage_health(
        &mut self,
        revision: u64,
        filesystems: taskmanager_core::core::storage_health::FilesystemHealthSnapshot,
        sources: Vec<SourceStatus>,
    ) {
        let _ = self
            .storage_health
            .replace(revision, Rc::new(filesystems), sources);
    }

    pub(super) const fn snapshot(&self) -> &Rc<SystemSnapshot> {
        &self.snapshot.value
    }

    pub(super) const fn snapshot_revision(&self) -> u64 {
        self.snapshot.revision
    }

    pub(super) const fn processes(&self) -> &Arc<Vec<ProcessItem>> {
        &self.processes.value
    }

    pub(super) const fn processes_revision(&self) -> u64 {
        self.processes.revision
    }

    pub(super) const fn running_process_count(&self) -> usize {
        self.running_process_count
    }

    pub(super) fn services(&self) -> &[ServiceItem] {
        self.services.materialized.value.as_slice()
    }

    pub(super) const fn services_rc(&self) -> &Rc<Vec<ServiceItem>> {
        &self.services.materialized.value
    }

    pub(super) const fn services_revision(&self) -> u64 {
        self.services.materialized.revision
    }

    pub(super) fn service_sources(&self) -> &[SourceStatus] {
        self.services.sources.as_slice()
    }

    pub(super) const fn service_sources_rc(&self) -> &Rc<Vec<SourceStatus>> {
        &self.services.sources
    }

    pub(super) fn startup_entries(&self) -> &[StartupEntry] {
        self.startup_entries.materialized.value.as_slice()
    }

    pub(super) const fn startup_entries_rc(&self) -> &Rc<Vec<StartupEntry>> {
        &self.startup_entries.materialized.value
    }

    pub(super) const fn startup_revision(&self) -> u64 {
        self.startup_entries.materialized.revision
    }

    pub(super) fn startup_sources(&self) -> &[SourceStatus] {
        self.startup_entries.sources.as_slice()
    }

    pub(super) const fn startup_sources_rc(&self) -> &Rc<Vec<SourceStatus>> {
        &self.startup_entries.sources
    }

    pub(super) fn sessions(&self) -> &[SessionItem] {
        self.sessions.materialized.value.as_slice()
    }

    pub(super) const fn sessions_rc(&self) -> &Rc<Vec<SessionItem>> {
        &self.sessions.materialized.value
    }

    pub(super) const fn sessions_revision(&self) -> u64 {
        self.sessions.materialized.revision
    }

    pub(super) fn session_sources(&self) -> &[SourceStatus] {
        self.sessions.sources.as_slice()
    }

    pub(super) const fn session_sources_rc(&self) -> &Rc<Vec<SourceStatus>> {
        &self.sessions.sources
    }

    pub(super) fn hardware(&self) -> &HardwareInfo {
        self.hardware.materialized.value.as_ref()
    }

    pub(super) const fn hardware_rc(&self) -> &Rc<HardwareInfo> {
        &self.hardware.materialized.value
    }

    pub(super) const fn hardware_revision(&self) -> u64 {
        self.hardware.materialized.revision
    }

    pub(super) fn hardware_sources(&self) -> &[SourceStatus] {
        self.hardware.sources.as_slice()
    }

    pub(super) const fn hardware_sources_rc(&self) -> &Rc<Vec<SourceStatus>> {
        &self.hardware.sources
    }

    pub(super) fn containers(&self) -> &ContainerRollup {
        self.containers.value.as_ref()
    }

    pub(super) fn startup_boot_evidence(&self) -> Option<&StartupBootEvidenceSnapshot> {
        self.startup_evidence.value.snapshot.as_ref()
    }

    pub(super) fn startup_evidence_failure(&self) -> Option<StartupEvidenceUnavailable> {
        self.startup_evidence.value.unavailable
    }

    pub(super) fn sensors(&self) -> &SensorCenterSnapshot {
        self.sensors.materialized.value.as_ref()
    }

    pub(super) fn sensor_sources(&self) -> &[SourceStatus] {
        self.sensors.sources.as_slice()
    }

    pub(super) fn power_supplies(&self) -> &PowerSupplySnapshot {
        self.power_supplies.materialized.value.as_ref()
    }

    pub(super) fn power_supply_sources(&self) -> &[SourceStatus] {
        self.power_supplies.sources.as_slice()
    }

    pub(super) fn directory_usage(&self) -> Option<&DirectoryUsageSnapshot> {
        self.directory_usage.value.as_deref()
    }

    pub(super) fn npu_inventory(&self) -> Option<&NpuInventorySnapshot> {
        self.npu_inventory.value.as_deref()
    }

    pub(super) fn active_alerts(&self) -> &[Alert] {
        self.active_alerts.value.as_slice()
    }

    pub(super) fn storage_health(
        &self,
    ) -> &taskmanager_core::core::storage_health::FilesystemHealthSnapshot {
        self.storage_health.materialized.value.as_ref()
    }

    pub(super) fn storage_health_sources(&self) -> &[SourceStatus] {
        self.storage_health.sources.as_slice()
    }
}

impl super::RootView {
    #[must_use]
    pub fn system_snapshot(&self) -> &SystemSnapshot {
        self.materialized.snapshot().as_ref()
    }

    #[must_use]
    pub fn system_snapshot_rc(&self) -> &Rc<SystemSnapshot> {
        self.materialized.snapshot()
    }

    #[must_use]
    pub const fn system_snapshot_generation(&self) -> u64 {
        self.materialized.snapshot_revision()
    }

    #[must_use]
    pub fn processes(&self) -> &[ProcessItem] {
        self.materialized.processes().as_ref().as_slice()
    }

    #[must_use]
    pub fn processes_arc(&self) -> &Arc<Vec<ProcessItem>> {
        self.materialized.processes()
    }

    #[must_use]
    pub const fn processes_generation(&self) -> u64 {
        self.materialized.processes_revision()
    }

    #[must_use]
    pub const fn running_process_count(&self) -> usize {
        self.materialized.running_process_count()
    }

    #[must_use]
    pub fn services(&self) -> &[ServiceItem] {
        self.materialized.services()
    }

    #[must_use]
    pub fn services_rc(&self) -> &Rc<Vec<ServiceItem>> {
        self.materialized.services_rc()
    }

    #[must_use]
    pub const fn services_generation(&self) -> u64 {
        self.materialized.services_revision()
    }

    #[must_use]
    pub fn service_sources(&self) -> &[SourceStatus] {
        self.materialized.service_sources()
    }

    #[must_use]
    pub fn service_sources_rc(&self) -> &Rc<Vec<SourceStatus>> {
        self.materialized.service_sources_rc()
    }

    #[must_use]
    pub fn startup_entries(&self) -> &[StartupEntry] {
        self.materialized.startup_entries()
    }

    #[must_use]
    pub fn startup_entries_rc(&self) -> &Rc<Vec<StartupEntry>> {
        self.materialized.startup_entries_rc()
    }

    #[must_use]
    pub const fn startup_generation(&self) -> u64 {
        self.materialized.startup_revision()
    }

    #[must_use]
    pub fn startup_sources(&self) -> &[SourceStatus] {
        self.materialized.startup_sources()
    }

    #[must_use]
    pub fn startup_sources_rc(&self) -> &Rc<Vec<SourceStatus>> {
        self.materialized.startup_sources_rc()
    }

    #[must_use]
    pub fn sessions(&self) -> &[SessionItem] {
        self.materialized.sessions()
    }

    #[must_use]
    pub fn sessions_rc(&self) -> &Rc<Vec<SessionItem>> {
        self.materialized.sessions_rc()
    }

    #[must_use]
    pub const fn sessions_generation(&self) -> u64 {
        self.materialized.sessions_revision()
    }

    #[must_use]
    pub fn session_sources(&self) -> &[SourceStatus] {
        self.materialized.session_sources()
    }

    #[must_use]
    pub fn session_sources_rc(&self) -> &Rc<Vec<SourceStatus>> {
        self.materialized.session_sources_rc()
    }

    #[must_use]
    pub fn hardware(&self) -> &HardwareInfo {
        self.materialized.hardware()
    }

    #[must_use]
    pub const fn hardware_rc(&self) -> &Rc<HardwareInfo> {
        self.materialized.hardware_rc()
    }

    #[must_use]
    pub const fn hardware_generation(&self) -> u64 {
        self.materialized.hardware_revision()
    }

    #[must_use]
    pub fn hardware_sources(&self) -> &[SourceStatus] {
        self.materialized.hardware_sources()
    }

    #[must_use]
    pub fn hardware_sources_rc(&self) -> &Rc<Vec<SourceStatus>> {
        self.materialized.hardware_sources_rc()
    }

    #[must_use]
    pub fn containers(&self) -> &ContainerRollup {
        self.materialized.containers()
    }

    #[must_use]
    pub fn startup_boot_evidence(&self) -> Option<&StartupBootEvidenceSnapshot> {
        self.materialized.startup_boot_evidence()
    }

    #[must_use]
    pub fn startup_evidence_failure(&self) -> Option<StartupEvidenceUnavailable> {
        self.materialized.startup_evidence_failure()
    }

    pub(crate) fn materialize_active_alerts(&mut self, revision: u64, alerts: Vec<Alert>) {
        self.materialized.replace_active_alerts(revision, alerts);
    }

    #[must_use]
    pub fn sensors(&self) -> &SensorCenterSnapshot {
        self.materialized.sensors()
    }

    #[must_use]
    pub fn sensor_sources(&self) -> &[SourceStatus] {
        self.materialized.sensor_sources()
    }

    #[must_use]
    pub fn power_supplies(&self) -> &PowerSupplySnapshot {
        self.materialized.power_supplies()
    }

    #[must_use]
    pub fn power_supply_sources(&self) -> &[SourceStatus] {
        self.materialized.power_supply_sources()
    }

    #[must_use]
    pub fn directory_usage(&self) -> Option<&DirectoryUsageSnapshot> {
        self.materialized.directory_usage()
    }

    #[must_use]
    pub fn npu_inventory(&self) -> Option<&NpuInventorySnapshot> {
        self.materialized.npu_inventory()
    }

    #[must_use]
    pub fn active_alerts(&self) -> &[Alert] {
        self.materialized.active_alerts()
    }

    #[must_use]
    pub fn storage_health(
        &self,
    ) -> &taskmanager_core::core::storage_health::FilesystemHealthSnapshot {
        self.materialized.storage_health()
    }

    #[must_use]
    pub fn storage_health_sources(&self) -> &[SourceStatus] {
        self.materialized.storage_health_sources()
    }

    /// Capture-only named system. The mutable projection value never escapes
    /// this boundary, so ordinary production modules cannot become a second
    /// materialization writer.
    pub(super) fn sync_capture_snapshot_system(&mut self) {
        let (capture_evidence, materialized, smart_history) = (
            &mut self.capture_evidence,
            &mut self.materialized,
            &mut self.smart_history,
        );
        let snapshot = Rc::make_mut(&mut materialized.snapshot.value);
        capture_evidence.on_snapshot(snapshot);
        smart_history.record_snapshot(snapshot);
    }

    pub(super) fn sync_capture_process_system(
        &mut self,
        processes_updated: bool,
    ) -> Option<super::CaptureProcessAction> {
        let (capture_evidence, materialized) = (&mut self.capture_evidence, &mut self.materialized);
        let action = capture_evidence.on_processes_update(
            processes_updated,
            Arc::make_mut(&mut materialized.processes.value),
        );
        if processes_updated {
            materialized.running_process_count = materialized
                .processes
                .value
                .iter()
                .filter(|process| process.status == "Running")
                .count();
        }
        action
    }

    pub(super) fn sync_capture_service_system(
        &mut self,
        services_updated: bool,
    ) -> Option<taskmanager_core::core::target::ServiceId> {
        let (capture_evidence, materialized) = (&mut self.capture_evidence, &mut self.materialized);
        capture_evidence.on_services_update(
            services_updated,
            Rc::make_mut(&mut materialized.services.materialized.value),
        )
    }

    pub(super) fn sync_capture_startup_system(
        &mut self,
        startup_updated: bool,
        restore_fixture: bool,
    ) -> bool {
        let (capture_evidence, materialized) = (&mut self.capture_evidence, &mut self.materialized);
        let entries = Rc::make_mut(&mut materialized.startup_entries.materialized.value);
        let evidence = &mut Rc::make_mut(&mut materialized.startup_evidence.value).snapshot;
        let ready = capture_evidence.on_startup_update(startup_updated, entries, evidence);
        if restore_fixture {
            capture_evidence.restore_startup_fixture(entries, evidence);
        }
        ready
    }

    pub(super) fn sync_capture_system_health_system(
        &mut self,
    ) -> super::capture::SystemHealthCaptureOutcome {
        let (capture_evidence, page, dashboard, materialized) = (
            &mut self.capture_evidence,
            &mut self.page,
            &mut self.dashboard,
            &mut self.materialized,
        );
        let sensors = Rc::make_mut(&mut materialized.sensors.materialized.value);
        let filesystems = Rc::make_mut(&mut materialized.storage_health.materialized.value);
        capture_evidence.on_system_health_state(
            page,
            dashboard,
            Rc::make_mut(&mut materialized.snapshot.value),
            filesystems,
            sensors,
        )
    }

    pub(super) fn sync_capture_dynamic_device_system(&mut self) -> bool {
        let (capture_evidence, page, materialized) = (
            &mut self.capture_evidence,
            &mut self.page,
            &mut self.materialized,
        );
        capture_evidence.on_dynamic_device_state(
            page,
            Rc::make_mut(&mut materialized.power_supplies.materialized.value),
            Rc::make_mut(&mut materialized.sensors.materialized.value),
        )
    }

    pub(super) fn sync_capture_live_dynamic_device_system(&mut self) -> bool {
        let (capture_evidence, page, materialized) = (
            &mut self.capture_evidence,
            &mut self.page,
            &self.materialized,
        );
        capture_evidence.on_live_dynamic_device_state(
            page,
            materialized.power_supplies.materialized.value.as_ref(),
        )
    }
}

#[cfg(any(test, feature = "test-support"))]
#[path = "../../../tests/gui/gpui_app/root/projection_materialization_test_support.rs"]
mod test_support;
