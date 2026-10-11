//! Fixture projection seeding through the shell-owned store reducers.

use std::sync::Arc;

use crate::fixture::{ProjectionSeedDomain, ProjectionSeedFact};

use super::{ShellApp, device_rows::DeviceRowFamily};

impl ShellApp {
    pub(crate) fn seed_fixture_fact(&mut self, fact: ProjectionSeedFact) {
        let is_process_fact = matches!(&fact, ProjectionSeedFact::Processes(_));
        match fact {
            ProjectionSeedFact::Snapshot(value) => {
                self.data.snapshot = *value;
                for family in [
                    DeviceRowFamily::Storage,
                    DeviceRowFamily::Network,
                    DeviceRowFamily::Gpu,
                ] {
                    self.data.note_device_row_family_changed(family);
                }
                self.data.system_revision = self.data.system_revision.saturating_add(1);
            }
            ProjectionSeedFact::Hardware(value) => {
                self.data.hardware = value.map(|value| *value);
                self.data.system_revision = self.data.system_revision.saturating_add(1);
            }
            ProjectionSeedFact::Processes(value) => {
                self.data.processes = value.map(Arc::new);
                self.data.processes_observed_at_ms = self
                    .data
                    .snapshot
                    .as_ref()
                    .map_or(0, |snapshot| snapshot.timestamp_ms);
                self.data.process_revision = self.data.process_revision.saturating_add(1);
            }
            ProjectionSeedFact::Services(value) => {
                self.data.services = value;
                self.data.services_revision = self.data.services_revision.saturating_add(1);
            }
            ProjectionSeedFact::StartupEntries(value) => {
                self.data.startup_entries = value;
                self.data.startup_revision = self.data.startup_revision.saturating_add(1);
            }
            ProjectionSeedFact::Sessions(value) => {
                self.data.sessions = value;
                self.data.sessions_revision = self.data.sessions_revision.saturating_add(1);
            }
            ProjectionSeedFact::Containers(value) => self.data.containers = value,
            ProjectionSeedFact::PowerSupplies(value) => {
                self.data.seed_power_supply_snapshot(value);
            }
            ProjectionSeedFact::Sensors(value) => {
                self.data.seed_sensor_snapshot(value);
            }
            ProjectionSeedFact::NpuInventory(value) => {
                if let Some(snapshot) = value {
                    self.data.seed_npu_inventory_fixture(snapshot);
                } else {
                    self.data.npu_inventory = None;
                    self.data
                        .note_device_row_family_changed(DeviceRowFamily::Npu);
                }
                self.data.system_revision = self.data.system_revision.saturating_add(1);
            }
            ProjectionSeedFact::DirectoryUsage(value) => self.data.directory_usage = value,
            ProjectionSeedFact::StartupBootEvidence(value) => {
                self.data.startup_boot_evidence = value;
            }
            ProjectionSeedFact::ServicesSource(value) => self.data.services_source = value,
            ProjectionSeedFact::StartupSource(value) => self.data.startup_source = value,
            ProjectionSeedFact::SessionsSource(value) => self.data.sessions_source = value,
            ProjectionSeedFact::ProcessAffinity(value) => {
                self.request_sessions.seed_affinity(value);
            }
            ProjectionSeedFact::ProcessInsights(value) => self.data.process_insights = *value,
            ProjectionSeedFact::ActiveAlerts(value) => self.data.alert_active = value,
            ProjectionSeedFact::AdvanceRevision(domain) => match domain {
                ProjectionSeedDomain::Processes => {
                    self.data.process_revision = self.data.process_revision.saturating_add(1);
                }
                ProjectionSeedDomain::Services => {
                    self.data.services_revision = self.data.services_revision.saturating_add(1);
                }
                ProjectionSeedDomain::Startup => {
                    self.data.startup_revision = self.data.startup_revision.saturating_add(1);
                }
                ProjectionSeedDomain::Sessions => {
                    self.data.sessions_revision = self.data.sessions_revision.saturating_add(1);
                }
                ProjectionSeedDomain::System => {
                    self.data.system_revision = self.data.system_revision.saturating_add(1);
                }
            },
            ProjectionSeedFact::AdvanceRefresh => {}
        }
        if is_process_fact {
            self.prune_stale_selection();
        }
        self.data.refresh_count = self.data.refresh_count.saturating_add(1);
    }
}
