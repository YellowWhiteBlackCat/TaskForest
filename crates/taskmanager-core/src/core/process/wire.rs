//! Private compatibility boundary for persisted process rows.

use std::path::PathBuf;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{
    ProcessApplicationIdentity, ProcessItem, ProcessMetadataAvailability,
    ProcessMetadataObservation, ProcessMetadataObservations, ProcessOwner, ProcessOwnerIdentity,
    ProcessScalarObservations,
};
use crate::core::{ScalarAvailability, ScalarObservation};

const LEGACY_METADATA_OBSERVED_AT_MS: u64 = 0;

/// Compatibility-only process snapshot. Owner/path strings exist only at the
/// serde boundary; the domain stores typed metadata observations.
#[derive(Serialize, Deserialize)]
struct ProcessItemWire {
    pid: u32,
    parent_pid: Option<u32>,
    name: String,
    cmdline: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cpu_usage: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    memory_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    disk_read_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    disk_write_bytes: Option<u64>,
    status: String,
    #[serde(default)]
    user: String,
    #[serde(default)]
    exe_path: Option<PathBuf>,
    #[serde(default)]
    metadata_observations: ProcessMetadataObservations,
    #[serde(default)]
    application_identity: ProcessMetadataObservation<ProcessApplicationIdentity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    threads: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    start_time_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cpu_time_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fds: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    nice: Option<i32>,
    #[serde(default)]
    scalar_observations: ProcessScalarObservations,
    cpu_history: Vec<f32>,
    mem_history: Vec<f32>,
    disk_history: Vec<f32>,
    disk_read_history: Vec<f32>,
    disk_write_history: Vec<f32>,
}

impl Serialize for ProcessItem {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ProcessItemWire {
            pid: self.pid,
            parent_pid: self.parent_pid,
            name: self.name.clone(),
            cmdline: self.cmdline.clone(),
            cpu_usage: self.current_cpu_percentage(),
            memory_bytes: self.current_memory_bytes(),
            disk_read_bytes: self.current_disk_read_bytes_per_sec(),
            disk_write_bytes: self.current_disk_write_bytes_per_sec(),
            status: self.status.clone(),
            user: self.current_user().unwrap_or_default(),
            exe_path: self.current_exe_path().map(PathBuf::from),
            metadata_observations: self.metadata_observations.clone(),
            application_identity: self.application_identity.clone(),
            threads: self.current_threads(),
            start_time_secs: self.current_start_time_secs(),
            cpu_time_secs: self.current_cpu_time_secs(),
            fds: self.current_fds(),
            nice: self.current_nice(),
            scalar_observations: self.scalar_observations,
            cpu_history: self.cpu_history.clone(),
            mem_history: self.mem_history.clone(),
            disk_history: self.disk_history.clone(),
            disk_read_history: self.disk_read_history.clone(),
            disk_write_history: self.disk_write_history.clone(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ProcessItem {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ProcessItemWire::deserialize(deserializer)?;
        let mut metadata_observations = wire.metadata_observations.clone();
        if legacy_metadata_identity_is_trusted(wire.pid, &wire.name, &wire.cmdline) {
            if metadata_observations.owner.availability() == ProcessMetadataAvailability::Unknown
                && !wire.user.trim().is_empty()
            {
                metadata_observations.owner = ProcessMetadataObservation::available(
                    ProcessOwner {
                        identity: ProcessOwnerIdentity::Opaque(wire.user.clone()),
                        label: None,
                    },
                    LEGACY_METADATA_OBSERVED_AT_MS,
                );
            }
            if metadata_observations.executable_path.availability()
                == ProcessMetadataAvailability::Unknown
                && let Some(path) = wire
                    .exe_path
                    .as_ref()
                    .filter(|path| !path.as_os_str().is_empty())
                    .cloned()
            {
                metadata_observations.executable_path =
                    ProcessMetadataObservation::available(path, LEGACY_METADATA_OBSERVED_AT_MS);
            }
        }
        let mut scalar_observations = wire.scalar_observations;
        hydrate_legacy_scalars(&mut scalar_observations, &wire);
        Ok(Self {
            pid: wire.pid,
            parent_pid: wire.parent_pid,
            name: wire.name,
            cmdline: wire.cmdline,
            status: wire.status,
            metadata_observations,
            application_identity: wire.application_identity,
            scalar_observations,
            cpu_history: wire.cpu_history,
            mem_history: wire.mem_history,
            disk_history: wire.disk_history,
            disk_read_history: wire.disk_read_history,
            disk_write_history: wire.disk_write_history,
        })
    }
}

fn hydrate_legacy_scalars(observations: &mut ProcessScalarObservations, wire: &ProcessItemWire) {
    if wire.pid > 0 {
        if is_unknown(&observations.cpu_percentage)
            && let Some(cpu_usage) = wire.cpu_usage.filter(|value| value.is_finite())
        {
            observations.cpu_percentage =
                ScalarObservation::available(cpu_usage, LEGACY_METADATA_OBSERVED_AT_MS);
        }
        if let Some(memory_bytes) = wire.memory_bytes {
            hydrate_unknown(&mut observations.memory_bytes, memory_bytes);
        }
        if let Some(disk_read_bytes) = wire.disk_read_bytes {
            hydrate_unknown(&mut observations.disk_read_bytes_per_sec, disk_read_bytes);
        }
        if let Some(disk_write_bytes) = wire.disk_write_bytes {
            hydrate_unknown(&mut observations.disk_write_bytes_per_sec, disk_write_bytes);
        }
        if let Some(threads) = wire.threads.filter(|value| *value > 0) {
            hydrate_unknown(&mut observations.threads, threads);
        }
        if let Some(start_time_secs) = wire.start_time_secs.filter(|value| *value > 0) {
            hydrate_unknown(&mut observations.start_time_secs, start_time_secs);
        }
    }

    if wire.pid > 0 && has_trustworthy_start_token(&observations.start_token) {
        if let Some(cpu_time_secs) = wire.cpu_time_secs {
            hydrate_unknown(&mut observations.cpu_time_secs, cpu_time_secs);
        }
        if let Some(fds) = wire.fds {
            hydrate_unknown(&mut observations.fds, fds);
        }
        if let Some(nice) = wire.nice {
            hydrate_unknown(&mut observations.nice, nice);
        }
    }
}

fn hydrate_unknown<T>(observation: &mut ScalarObservation<T>, value: T) {
    if is_unknown(observation) {
        *observation = ScalarObservation::available(value, LEGACY_METADATA_OBSERVED_AT_MS);
    }
}

fn is_unknown<T>(observation: &ScalarObservation<T>) -> bool {
    observation.availability() == ScalarAvailability::Unknown
}

fn has_trustworthy_start_token(observation: &ScalarObservation<u64>) -> bool {
    observation.availability() == ScalarAvailability::Available
        && observation.current_value().is_some_and(|value| *value > 0)
        && observation.last_success_ms().is_some()
}

fn legacy_metadata_identity_is_trusted(pid: u32, name: &str, cmdline: &str) -> bool {
    pid > 0 && (!name.trim().is_empty() || !cmdline.trim().is_empty())
}
