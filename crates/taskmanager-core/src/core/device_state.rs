//! Platform-neutral device identity and lifecycle state.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::{DeviceGeneration, DeviceId, FailureKind, SourceOutcome};

/// Grace period during which confirmed-absent device identities remain
/// available for selection recovery and diagnostics.
pub const DEFAULT_DEVICE_ABSENCE_RETENTION_MS: u64 = 30_000;

/// Hard ceiling on tracked stable device identities.
///
/// Real machines track far fewer than 1024 disks, NICs and GPUs. The bound
/// exists for providers whose stable identity churns (random veth MACs,
/// re-serialized disks): those dead identities would otherwise accumulate
/// without bound inside the 30-second absence window. When an observation
/// would exceed the ceiling, the registry first forgets the identity that
/// has been confirmed absent the longest — or, absent none, the least
/// recently seen one — so memory stays finite and one present device keeps
/// exactly one authority record.
pub const MAX_TRACKED_DEVICE_IDENTITIES: usize = 1_024;

/// A small, actionable state vocabulary shared by disks, NICs, GPUs and
/// auxiliary providers such as SMART.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DeviceStatus {
    Healthy,
    Stale,
    PermissionDenied,
    MissingTool,
    #[default]
    Unsupported,
}

impl DeviceStatus {
    /// Convert a provider/source failure into device health without parsing
    /// platform error text.
    #[must_use]
    pub const fn from_failure(failure: FailureKind) -> Self {
        match failure {
            // RequiresEscalation is an escalatable capability denial (the Intel
            // PMU under perf_event_paranoid), so at the coarse device-health
            // layer it is a PermissionDenied state; the finer FailureKind is
            // preserved in the typed failure channel where the UI can prompt.
            FailureKind::PermissionDenied | FailureKind::RequiresEscalation => {
                Self::PermissionDenied
            }
            FailureKind::MissingDependency => Self::MissingTool,
            FailureKind::Unsupported => Self::Unsupported,
            FailureKind::TimedOut
            | FailureKind::IdentityChanged
            | FailureKind::TemporarilyUnavailable
            | FailureKind::Rejected
            | FailureKind::ProviderFault => Self::Stale,
        }
    }

    /// Return the stable failure vocabulary for a non-healthy device state.
    #[must_use]
    pub const fn failure(self) -> Option<FailureKind> {
        match self {
            Self::Healthy => None,
            Self::Stale => Some(FailureKind::TemporarilyUnavailable),
            Self::PermissionDenied => Some(FailureKind::PermissionDenied),
            Self::MissingTool => Some(FailureKind::MissingDependency),
            Self::Unsupported => Some(FailureKind::Unsupported),
        }
    }

    /// Single severity lattice for aggregating mixed device states:
    /// `PermissionDenied > MissingTool > Stale > Unsupported > Healthy`.
    /// Unavailable observations dominate healthy ones, so a healthy channel
    /// never masks an unavailable one in the same rollup.
    #[must_use]
    pub const fn severity(self) -> u8 {
        match self {
            Self::PermissionDenied => 4,
            Self::MissingTool => 3,
            Self::Stale => 2,
            Self::Unsupported => 1,
            Self::Healthy => 0,
        }
    }
}

/// Current status plus the last wall-clock millisecond at which trustworthy
/// telemetry was collected. Failures never erase the last successful time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DeviceState {
    pub status: DeviceStatus,
    pub last_success_ms: Option<u64>,
}

impl DeviceState {
    pub const fn healthy(now_ms: u64) -> Self {
        Self {
            status: DeviceStatus::Healthy,
            last_success_ms: Some(now_ms),
        }
    }

    pub const fn transition(self, observed: DeviceStatus, now_ms: u64) -> Self {
        if matches!(observed, DeviceStatus::Healthy) {
            Self {
                status: DeviceStatus::Healthy,
                last_success_ms: match self.last_success_ms {
                    Some(previous) if previous > now_ms => Some(previous),
                    _ => Some(now_ms),
                },
            }
        } else {
            Self {
                status: observed,
                last_success_ms: self.last_success_ms,
            }
        }
    }

    /// Merge a provider observation without allowing its timestamp to erase or
    /// move the previously trustworthy success marker backwards.
    pub const fn merge_observation(self, observed: Self, now_ms: u64) -> Self {
        let previous_success = match (self.last_success_ms, observed.last_success_ms) {
            (Some(left), Some(right)) => Some(if left > right { left } else { right }),
            (Some(value), None) | (None, Some(value)) => Some(value),
            (None, None) => None,
        };
        if matches!(observed.status, DeviceStatus::Healthy) {
            let last_success_ms = match previous_success {
                Some(previous) if previous > now_ms => Some(previous),
                _ => Some(now_ms),
            };
            Self {
                status: DeviceStatus::Healthy,
                last_success_ms,
            }
        } else {
            Self {
                status: observed.status,
                last_success_ms: previous_success,
            }
        }
    }
}

/// Presence is orthogonal to telemetry health. A device can be present while
/// its metrics are stale or permission denied. `Unavailable` means discovery
/// failed, so the provider cannot truthfully claim either presence or absence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DevicePresence {
    Present,
    Absent,
    #[default]
    Unavailable,
}

/// Platform-neutral lifecycle record keyed by a stable device identity.
///
/// A generation starts at one and advances only after a confirmed
/// `Absent -> Present` transition. A provider outage is not evidence of device
/// removal and therefore never advances the generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DeviceLifecycle {
    pub presence: DevicePresence,
    pub state: DeviceState,
    pub generation: DeviceGeneration,
    pub first_seen_ms: Option<u64>,
    pub last_seen_ms: Option<u64>,
    pub absent_since_ms: Option<u64>,
}

/// Whether a discovery refresh completed authoritatively.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceRefreshOutcome {
    /// Every device visible to the provider was supplied to `observe`.
    Complete,
    /// Discovery could not decide which devices are present. The status must
    /// describe the provider failure (normally stale or permission denied).
    Unavailable(DeviceStatus),
}

impl DeviceRefreshOutcome {
    /// Derive lifecycle authority from the explicitly selected discovery
    /// source. Optional enrichment sources must never be passed here.
    #[must_use]
    pub const fn from_discovery_outcome(outcome: SourceOutcome) -> Self {
        match outcome {
            SourceOutcome::Available | SourceOutcome::Empty => Self::Complete,
            SourceOutcome::Partial(failure) | SourceOutcome::Unavailable(failure) => {
                Self::Unavailable(DeviceStatus::from_failure(failure))
            }
        }
    }
}

/// Changes emitted when a lifecycle refresh is closed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeviceLifecycleDelta {
    /// Stable IDs newly confirmed absent during this refresh.
    pub newly_absent: Vec<DeviceId>,
    /// Stable IDs that returned after confirmed absence and started a new
    /// generation. Generation-scoped rate/history caches must reset for these.
    pub reappeared: Vec<DeviceId>,
    /// Stable IDs whose confirmed-absence grace period elapsed.
    pub expired: Vec<DeviceId>,
}

/// Stateful reconciliation for hot-pluggable devices.
///
/// Call [`begin_refresh`](Self::begin_refresh), then
/// [`observe`](Self::observe) once per discovered stable ID, and finally
/// [`finish_refresh`](Self::finish_refresh). Confirmed-absent entries remain
/// queryable through the configured retention period so selection and
/// diagnostics can explain disappearance; they are evicted only when
/// `absence_age > retention_ms`.
#[derive(Debug, Clone)]
pub struct DeviceLifecycleRegistry {
    retention_ms: u64,
    entries: HashMap<DeviceId, DeviceLifecycle>,
    observed: HashSet<DeviceId>,
    reappeared: HashSet<DeviceId>,
}

impl DeviceLifecycleRegistry {
    #[must_use]
    pub fn new(retention_ms: u64) -> Self {
        Self {
            retention_ms,
            entries: HashMap::new(),
            observed: HashSet::new(),
            reappeared: HashSet::new(),
        }
    }

    pub fn begin_refresh(&mut self) {
        self.observed.clear();
        self.reappeared.clear();
    }

    /// Record one present device and return its merged lifecycle.
    ///
    /// Duplicate observations in a refresh are harmless. Callers should still
    /// deduplicate hardware identities before merging measurements. Insertion
    /// is capped at [`MAX_TRACKED_DEVICE_IDENTITIES`]: a new identity beyond
    /// the ceiling evicts the longest-confirmed-absent entry first, then the
    /// least recently seen one (see the constant's documentation).
    pub fn observe(
        &mut self,
        stable_id: impl Into<DeviceId>,
        observed_state: DeviceState,
        now_ms: u64,
    ) -> DeviceLifecycle {
        let stable_id = stable_id.into();
        self.observed.insert(stable_id.clone());
        if !self.entries.contains_key(&stable_id) {
            self.evict_beyond_ceiling();
        }
        let lifecycle = self
            .entries
            .entry(stable_id.clone())
            .or_insert_with(|| DeviceLifecycle {
                presence: DevicePresence::Present,
                state: DeviceState::default().merge_observation(observed_state, now_ms),
                generation: DeviceGeneration::INITIAL,
                first_seen_ms: Some(now_ms),
                last_seen_ms: Some(now_ms),
                absent_since_ms: None,
            });

        if lifecycle.presence == DevicePresence::Absent {
            lifecycle.generation = lifecycle.generation.next();
            self.reappeared.insert(stable_id);
        }
        lifecycle.presence = DevicePresence::Present;
        lifecycle.state = lifecycle.state.merge_observation(observed_state, now_ms);
        lifecycle.first_seen_ms = lifecycle.first_seen_ms.or(Some(now_ms));
        lifecycle.last_seen_ms = Some(
            lifecycle
                .last_seen_ms
                .map_or(now_ms, |previous| previous.max(now_ms)),
        );
        lifecycle.absent_since_ms = None;
        *lifecycle
    }

    /// Make room for one new identity when the registry is saturated.
    ///
    /// Confirmed-absent identities are evicted first, oldest absence wins;
    /// with no absent entry the least recently seen identity is dropped. The
    /// stable-ID string is the deterministic tie-break so eviction never
    /// depends on hash order. Identities the size of the ceiling or below
    /// never lose an entry to this path.
    fn evict_beyond_ceiling(&mut self) {
        if self.entries.len() < MAX_TRACKED_DEVICE_IDENTITIES {
            return;
        }
        let victim = self
            .entries
            .iter()
            .min_by_key(|(id, lifecycle)| {
                let absent_rank = u64::from(lifecycle.absent_since_ms.is_none());
                (
                    absent_rank,
                    lifecycle.absent_since_ms.or(lifecycle.last_seen_ms),
                    lifecycle.last_seen_ms,
                    lifecycle.first_seen_ms,
                    id.as_str(),
                )
            })
            .map(|(id, _)| (*id).clone());
        if let Some(victim) = victim {
            self.entries.remove(&victim);
        }
    }

    /// Close the refresh, distinguish confirmed absence from discovery
    /// unavailability, and prune entries beyond the absence retention period.
    pub fn finish_refresh(
        &mut self,
        outcome: DeviceRefreshOutcome,
        now_ms: u64,
    ) -> DeviceLifecycleDelta {
        let mut delta = DeviceLifecycleDelta::default();
        for (id, lifecycle) in &mut self.entries {
            if self.observed.contains(id) {
                continue;
            }
            match outcome {
                DeviceRefreshOutcome::Complete => {
                    if lifecycle.presence != DevicePresence::Absent {
                        lifecycle.presence = DevicePresence::Absent;
                        lifecycle.absent_since_ms = Some(now_ms);
                        lifecycle.state = lifecycle.state.transition(DeviceStatus::Stale, now_ms);
                        delta.newly_absent.push(id.clone());
                    }
                }
                DeviceRefreshOutcome::Unavailable(status) => {
                    if lifecycle.presence != DevicePresence::Absent {
                        lifecycle.presence = DevicePresence::Unavailable;
                        lifecycle.absent_since_ms = None;
                        lifecycle.state = lifecycle.state.transition(status, now_ms);
                    }
                }
            }
        }

        self.entries.retain(|id, lifecycle| {
            let expired = lifecycle.presence == DevicePresence::Absent
                && lifecycle.absent_since_ms.is_some_and(|absent_since| {
                    now_ms.saturating_sub(absent_since) > self.retention_ms
                });
            if expired {
                delta.expired.push(id.clone());
            }
            !expired
        });
        delta.reappeared.extend(self.reappeared.drain());
        self.observed.clear();
        delta.newly_absent.sort();
        delta.reappeared.sort();
        delta.expired.sort();
        delta
    }

    #[must_use]
    pub fn get(&self, stable_id: &str) -> Option<&DeviceLifecycle> {
        self.entries.get(stable_id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &DeviceLifecycle)> {
        self.entries
            .iter()
            .map(|(id, lifecycle)| (id.as_str(), lifecycle))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Stable selection survives temporary absence: `resolve` returns `None` while
/// the chosen ID is missing and resolves the same ID again after re-add.
#[derive(Debug, Clone, Default)]
pub struct StableDeviceSelection {
    selected_id: Option<DeviceId>,
}

impl StableDeviceSelection {
    pub fn select(&mut self, id: impl Into<DeviceId>) {
        self.selected_id = Some(id.into());
    }

    pub fn selected_id(&self) -> Option<&str> {
        self.selected_id.as_ref().map(DeviceId::as_str)
    }

    pub fn clear(&mut self) {
        self.selected_id = None;
    }

    pub fn resolve<'a>(&self, ids: impl IntoIterator<Item = &'a str>) -> Option<usize> {
        let selected = self.selected_id.as_ref()?.as_str();
        ids.into_iter().position(|id| id == selected)
    }
}

fn clean_identity(value: &str) -> Option<String> {
    let cleaned = value
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '.' | '_' | '-'))
        .collect::<String>()
        .to_ascii_lowercase();
    (!cleaned.is_empty()).then_some(cleaned)
}

pub fn stable_disk_id(name: &str, wwid: Option<&str>, serial: Option<&str>) -> String {
    if let Some(id) = wwid.and_then(clean_identity) {
        format!("disk:wwid:{id}")
    } else if let Some(id) = serial.and_then(clean_identity) {
        format!("disk:serial:{id}")
    } else {
        format!(
            "disk:path:{}",
            clean_identity(name).unwrap_or_else(|| "unknown".into())
        )
    }
}

pub fn stable_network_id(name: &str, mac: Option<&str>) -> String {
    mac.and_then(clean_identity)
        .map(|id| format!("net:mac:{id}"))
        .unwrap_or_else(|| {
            format!(
                "net:name:{}",
                clean_identity(name).unwrap_or_else(|| "unknown".into())
            )
        })
}

pub fn stable_gpu_id(card_name: &str, pci_slot: Option<&str>) -> String {
    pci_slot
        .and_then(clean_identity)
        .map(|id| format!("gpu:pci:{id}"))
        .unwrap_or_else(|| {
            format!(
                "gpu:drm:{}",
                clean_identity(card_name).unwrap_or_else(|| "unknown".into())
            )
        })
}

#[cfg(test)]
#[path = "../../tests/headless/core_core_device_state_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/headless/core_core_device_state_stable_identity_tests.rs"]
mod stable_identity_tests;

#[cfg(test)]
#[path = "../../tests/headless/core_core_device_state_state_merge_tests.rs"]
mod state_merge_tests;
