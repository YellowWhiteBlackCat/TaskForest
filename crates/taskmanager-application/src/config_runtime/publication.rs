//! Immutable configuration publications and the bounded per-process replay log.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use taskmanager_core::config::Config;

use crate::{ConfigLoadResult, ConfigLoadSource, ConfigStoreError, ConfigStoreErrorKind};

/// Monotonic revision of the last successfully published canonical snapshot.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConfigRevision(pub(super) u64);

impl ConfigRevision {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    pub(super) const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

/// Monotonic identity of every publication, including failures that do not
/// advance [`ConfigRevision`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConfigPublicationId(u64);

impl ConfigPublicationId {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Typed recovery evidence attached to initial and external refresh loads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigRecovery {
    source: ConfigLoadSource,
    primary_error: Option<ConfigStoreErrorKind>,
    backup_error: Option<ConfigStoreErrorKind>,
}

/// User-visible classification of an initial configuration load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigRecoveryNotice {
    None,
    Recovered,
    Failed,
}

impl ConfigRecovery {
    #[must_use]
    pub const fn source(self) -> ConfigLoadSource {
        self.source
    }

    #[must_use]
    pub const fn primary_error(self) -> Option<ConfigStoreErrorKind> {
        self.primary_error
    }

    #[must_use]
    pub const fn backup_error(self) -> Option<ConfigStoreErrorKind> {
        self.backup_error
    }

    /// A normal first launch: neither primary nor backup exists yet.
    #[must_use]
    pub const fn is_pristine_default(self) -> bool {
        matches!(
            self,
            Self {
                source: ConfigLoadSource::Default,
                primary_error: Some(ConfigStoreErrorKind::Missing),
                backup_error: Some(ConfigStoreErrorKind::Missing),
            }
        )
    }

    /// A pristine first launch is deliberately silent while damaged-primary
    /// recovery remains observable.
    #[must_use]
    pub fn initial_notice(self) -> ConfigRecoveryNotice {
        if self.source == ConfigLoadSource::Primary || self.is_pristine_default() {
            ConfigRecoveryNotice::None
        } else if self.source == ConfigLoadSource::Backup {
            ConfigRecoveryNotice::Recovered
        } else {
            ConfigRecoveryNotice::Failed
        }
    }

    pub(super) const fn primary() -> Self {
        Self {
            source: ConfigLoadSource::Primary,
            primary_error: None,
            backup_error: None,
        }
    }
}

impl From<&ConfigLoadResult> for ConfigRecovery {
    fn from(result: &ConfigLoadResult) -> Self {
        Self {
            source: result.source(),
            primary_error: result.primary_error(),
            backup_error: result.backup_error(),
        }
    }
}

/// Why one immutable configuration snapshot was published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigPublicationOutcome {
    Loaded(ConfigRecovery),
    Saved {
        accepted_base: ConfigRevision,
    },
    Refreshed(ConfigRecovery),
    RefreshFailed(ConfigRecovery),
    SaveFailed {
        accepted_base: ConfigRevision,
        error: ConfigStoreError,
    },
}

impl ConfigPublicationOutcome {
    #[must_use]
    pub const fn is_failure(&self) -> bool {
        matches!(self, Self::RefreshFailed(_) | Self::SaveFailed { .. })
    }
}

/// One full immutable publication. Failures retain the prior canonical
/// snapshot and revision so renderers can report the error without resetting
/// their applied preferences.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigPublication {
    id: ConfigPublicationId,
    revision: ConfigRevision,
    snapshot: Arc<Config>,
    outcome: ConfigPublicationOutcome,
}

impl ConfigPublication {
    #[must_use]
    pub const fn id(&self) -> ConfigPublicationId {
        self.id
    }

    #[must_use]
    pub const fn revision(&self) -> ConfigRevision {
        self.revision
    }

    #[must_use]
    pub fn snapshot(&self) -> &Arc<Config> {
        &self.snapshot
    }

    #[must_use]
    pub const fn outcome(&self) -> &ConfigPublicationOutcome {
        &self.outcome
    }
}

/// Bounded replay result for one client cursor.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigDrain {
    Empty,
    Publications(Vec<Arc<ConfigPublication>>),
    /// The latest full snapshot makes lag recovery explicit rather than
    /// pretending skipped revisions were observed.
    ResyncRequired {
        missed_publications: u64,
        latest: Arc<ConfigPublication>,
    },
}

impl ConfigDrain {
    #[must_use]
    pub fn latest(&self) -> Option<&Arc<ConfigPublication>> {
        match self {
            Self::Empty => None,
            Self::Publications(publications) => publications.last(),
            Self::ResyncRequired { latest, .. } => Some(latest),
        }
    }
}

#[derive(Debug)]
struct PublicationBuffer {
    capacity: usize,
    next_id: u64,
    log: VecDeque<Arc<ConfigPublication>>,
}

#[derive(Debug)]
pub(super) struct PublicationState {
    buffer: Mutex<PublicationBuffer>,
    changed: Condvar,
}

impl PublicationState {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            buffer: Mutex::new(PublicationBuffer {
                capacity,
                next_id: 1,
                log: VecDeque::with_capacity(capacity),
            }),
            changed: Condvar::new(),
        }
    }

    pub(super) fn publish(
        &self,
        revision: ConfigRevision,
        snapshot: Arc<Config>,
        outcome: ConfigPublicationOutcome,
    ) {
        let mut buffer = lock_unpoisoned(&self.buffer);
        let id = ConfigPublicationId(buffer.next_id);
        buffer.next_id = buffer.next_id.saturating_add(1);
        buffer.log.push_back(Arc::new(ConfigPublication {
            id,
            revision,
            snapshot,
            outcome,
        }));
        while buffer.log.len() > buffer.capacity {
            buffer.log.pop_front();
        }
        drop(buffer);
        self.changed.notify_all();
    }

    pub(super) fn drain_after(&self, cursor: u64) -> ConfigDrain {
        let buffer = lock_unpoisoned(&self.buffer);
        let Some(oldest) = buffer.log.front() else {
            return ConfigDrain::Empty;
        };
        let expected = cursor.saturating_add(1);
        if expected < oldest.id.get() {
            let Some(latest) = buffer.log.back().cloned() else {
                return ConfigDrain::Empty;
            };
            return ConfigDrain::ResyncRequired {
                missed_publications: oldest.id.get().saturating_sub(expected),
                latest,
            };
        }
        let publications = buffer
            .log
            .iter()
            .filter(|publication| publication.id.get() > cursor)
            .cloned()
            .collect::<Vec<_>>();
        if publications.is_empty() {
            ConfigDrain::Empty
        } else {
            ConfigDrain::Publications(publications)
        }
    }

    pub(super) fn wait_for_change(&self, cursor: u64, timeout: Duration) {
        let buffer = lock_unpoisoned(&self.buffer);
        let guard = self
            .changed
            .wait_timeout_while(buffer, timeout, |buffer| {
                buffer
                    .log
                    .back()
                    .is_none_or(|publication| publication.id.get() <= cursor)
            })
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .0;
        drop(guard);
    }

    pub(super) fn wait_for_latest(
        &self,
        timeout: Duration,
    ) -> (Option<Arc<ConfigPublication>>, bool) {
        let buffer = lock_unpoisoned(&self.buffer);
        let (buffer, timeout_result) = self
            .changed
            .wait_timeout_while(buffer, timeout, |buffer| buffer.log.is_empty())
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (buffer.log.back().cloned(), timeout_result.timed_out())
    }

    pub(super) fn notify_all(&self) {
        self.changed.notify_all();
    }
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
