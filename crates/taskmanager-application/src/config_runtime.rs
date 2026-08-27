//! Bounded background authority for shared configuration revisions.
//!
//! [`ConfigStore`] remains the filesystem transaction primitive. This module
//! owns the one background worker that calls it, accepts client-local
//! base-to-local patches, and publishes immutable canonical snapshots. UI
//! event loops only use non-blocking submit/drain methods.

use std::fmt;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, TrySendError, bounded, select, tick};
use taskmanager_core::config::Config;

use crate::{ConfigLoadSource, ConfigStore};

mod publication;

use publication::PublicationState;
pub use publication::{
    ConfigDrain, ConfigPublication, ConfigPublicationId, ConfigPublicationOutcome, ConfigRecovery,
    ConfigRecoveryNotice, ConfigRevision,
};

/// Production command-lane bound. Every queued item owns at most one bounded
/// configuration document; overload is reported synchronously to the caller.
pub const DEFAULT_CONFIG_COMMAND_CAPACITY: usize = 16;
/// Production publication replay bound shared by all clients in one process.
pub const DEFAULT_CONFIG_PUBLICATION_CAPACITY: usize = 32;
/// Background cadence for observing cooperative external writers.
pub const DEFAULT_CONFIG_REFRESH_INTERVAL: Duration = Duration::from_secs(1);
/// Composition-edge wait bound used when the first window needs persisted
/// appearance before it is created.
pub const DEFAULT_CONFIG_INITIAL_WAIT: Duration = Duration::from_secs(1);

/// Result of a non-blocking client submission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSubmissionStatus {
    Queued,
    NoChange,
}

/// Typed rejection from the bounded command lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSubmitError {
    NotReady,
    Backpressure,
    Stopped,
}

impl fmt::Display for ConfigSubmitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotReady => "configuration runtime is not ready",
            Self::Backpressure => "configuration command lane is full",
            Self::Stopped => "configuration runtime has stopped",
        })
    }
}

impl std::error::Error for ConfigSubmitError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSynchronizeError {
    Backpressure,
    Stopped,
    TimedOut,
}

/// Typed source used when a bounded initial wait cannot return a worker
/// publication. The fallback snapshot is always the canonical default, never
/// an unclassified fabricated load success.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigBootstrapFallback {
    TimedOutDefault,
    WorkerStoppedDefault,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigBootstrap {
    Published(Arc<ConfigPublication>),
    Fallback {
        snapshot: Arc<Config>,
        source: ConfigBootstrapFallback,
    },
}

impl ConfigBootstrap {
    #[must_use]
    pub fn snapshot(&self) -> &Arc<Config> {
        match self {
            Self::Published(publication) => publication.snapshot(),
            Self::Fallback { snapshot, .. } => snapshot,
        }
    }
}

/// Runtime capacities and external-refresh cadence. Capacities are clamped to
/// at least one so every configured runtime remains live and bounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigRuntimeOptions {
    pub command_capacity: usize,
    pub publication_capacity: usize,
    pub refresh_interval: Duration,
}

impl Default for ConfigRuntimeOptions {
    fn default() -> Self {
        Self {
            command_capacity: DEFAULT_CONFIG_COMMAND_CAPACITY,
            publication_capacity: DEFAULT_CONFIG_PUBLICATION_CAPACITY,
            refresh_interval: DEFAULT_CONFIG_REFRESH_INTERVAL,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigRuntimeStartError {
    detail: String,
}

impl ConfigRuntimeStartError {
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for ConfigRuntimeStartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("configuration worker could not start")
    }
}

impl std::error::Error for ConfigRuntimeStartError {}

/// Observable worker lifecycle used by composition shutdown and deterministic
/// behavior tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigWorkerState {
    Starting,
    Running,
    Stopped,
}

#[derive(Debug)]
struct WorkerLifecycle {
    state: Mutex<ConfigWorkerState>,
    changed: Condvar,
}

#[derive(Debug, Clone)]
pub struct ConfigRuntimeMonitor {
    lifecycle: Arc<WorkerLifecycle>,
}

impl ConfigRuntimeMonitor {
    #[must_use]
    pub fn state(&self) -> ConfigWorkerState {
        *lock_unpoisoned(&self.lifecycle.state)
    }

    #[must_use]
    pub fn wait_stopped(&self, timeout: Duration) -> bool {
        let state = lock_unpoisoned(&self.lifecycle.state);
        let state = self
            .lifecycle
            .changed
            .wait_timeout_while(state, timeout, |state| *state != ConfigWorkerState::Stopped)
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .0;
        *state == ConfigWorkerState::Stopped
    }
}

#[derive(Debug)]
enum ConfigCommand {
    Submit {
        accepted_base: ConfigRevision,
        base: Arc<Config>,
        local: Box<Config>,
    },
    Refresh,
    Barrier(Sender<()>),
}

#[derive(Debug)]
struct RuntimeInner {
    commands: Sender<ConfigCommand>,
    shutdown: Sender<()>,
    publications: Arc<PublicationState>,
    lifecycle: Arc<WorkerLifecycle>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Debug)]
struct RuntimeHandle {
    inner: Arc<RuntimeInner>,
}

impl Clone for RuntimeHandle {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl Drop for RuntimeHandle {
    fn drop(&mut self) {
        if Arc::strong_count(&self.inner) != 1 {
            return;
        }
        // Shutdown has an independent bounded control seam. It cannot wait
        // behind a full configuration command lane.
        let _ = self.inner.shutdown.try_send(());
        if let Some(worker) = lock_unpoisoned(&self.inner.worker).take() {
            let _ = worker.join();
        }
    }
}

/// Owner/factory retained by the native composition root.
#[derive(Debug, Clone)]
pub struct ConfigCoordinator {
    handle: RuntimeHandle,
}

impl ConfigCoordinator {
    pub fn start_path(
        path: impl Into<std::path::PathBuf>,
    ) -> Result<Self, ConfigRuntimeStartError> {
        Self::start(ConfigStore::new(path))
    }

    pub fn start(store: ConfigStore) -> Result<Self, ConfigRuntimeStartError> {
        Self::start_with_options(store, ConfigRuntimeOptions::default())
    }

    pub fn start_with_options(
        store: ConfigStore,
        options: ConfigRuntimeOptions,
    ) -> Result<Self, ConfigRuntimeStartError> {
        let command_capacity = options.command_capacity.max(1);
        let publication_capacity = options.publication_capacity.max(1);
        let refresh_interval = options.refresh_interval.max(Duration::from_millis(1));
        let (commands, command_rx) = bounded(command_capacity);
        let (shutdown, shutdown_rx) = bounded(1);
        let publications = Arc::new(PublicationState::new(publication_capacity));
        let lifecycle = Arc::new(WorkerLifecycle {
            state: Mutex::new(ConfigWorkerState::Starting),
            changed: Condvar::new(),
        });
        let worker_publications = publications.clone();
        let worker_lifecycle = lifecycle.clone();
        let worker = thread::Builder::new()
            .name("taskforest-config".to_owned())
            .spawn(move || {
                config_worker(
                    store,
                    command_rx,
                    shutdown_rx,
                    worker_publications,
                    worker_lifecycle,
                    refresh_interval,
                );
            })
            .map_err(|error| ConfigRuntimeStartError {
                detail: error.to_string(),
            })?;
        Ok(Self {
            handle: RuntimeHandle {
                inner: Arc::new(RuntimeInner {
                    commands,
                    shutdown,
                    publications,
                    lifecycle,
                    worker: Mutex::new(Some(worker)),
                }),
            },
        })
    }

    #[must_use]
    pub fn client(&self) -> ConfigClient {
        ConfigClient {
            handle: self.handle.clone(),
            cursor: 0,
            observed: None,
        }
    }

    #[must_use]
    pub fn monitor(&self) -> ConfigRuntimeMonitor {
        ConfigRuntimeMonitor {
            lifecycle: self.handle.inner.lifecycle.clone(),
        }
    }
}

/// One window/client view of the shared coordinator. Its cursor and observed
/// base are independent; cloning a native host creates another client through
/// [`ConfigCoordinator::client`] instead of sharing mutable cursor state.
#[derive(Debug)]
pub struct ConfigClient {
    handle: RuntimeHandle,
    cursor: u64,
    observed: Option<(ConfigRevision, Arc<Config>)>,
}

impl ConfigClient {
    #[must_use]
    pub fn revision(&self) -> Option<ConfigRevision> {
        self.observed.as_ref().map(|(revision, _)| *revision)
    }

    #[must_use]
    pub fn snapshot(&self) -> Option<&Arc<Config>> {
        self.observed.as_ref().map(|(_, snapshot)| snapshot)
    }

    /// Queue one client-local full snapshot relative to the last publication
    /// this client observed. The worker computes and commits only the changed
    /// top-level fields.
    pub fn try_submit(&self, local: Config) -> Result<ConfigSubmissionStatus, ConfigSubmitError> {
        let Some((accepted_base, base)) = &self.observed else {
            return Err(ConfigSubmitError::NotReady);
        };
        if base.as_ref() == &local {
            return Ok(ConfigSubmissionStatus::NoChange);
        }
        self.handle
            .inner
            .commands
            .try_send(ConfigCommand::Submit {
                accepted_base: *accepted_base,
                base: base.clone(),
                local: Box::new(local),
            })
            .map(|()| ConfigSubmissionStatus::Queued)
            .map_err(map_try_send_error)
    }

    /// Ask the worker to observe the current disk snapshot immediately. The
    /// normal monotonic refresh cadence remains active independently.
    pub fn try_refresh(&self) -> Result<(), ConfigSubmitError> {
        self.handle
            .inner
            .commands
            .try_send(ConfigCommand::Refresh)
            .map_err(map_try_send_error)
    }

    /// Wait until every command accepted before this barrier has completed.
    /// This composition/test seam never performs I/O itself and must not be
    /// called from renderer event or tick paths.
    pub fn synchronize(&self, timeout: Duration) -> Result<(), ConfigSynchronizeError> {
        let (ack, ack_rx) = bounded(1);
        self.handle
            .inner
            .commands
            .try_send(ConfigCommand::Barrier(ack))
            .map_err(|error| match error {
                TrySendError::Full(_) => ConfigSynchronizeError::Backpressure,
                TrySendError::Disconnected(_) => ConfigSynchronizeError::Stopped,
            })?;
        ack_rx.recv_timeout(timeout).map_err(|error| match error {
            RecvTimeoutError::Timeout => ConfigSynchronizeError::TimedOut,
            RecvTimeoutError::Disconnected => ConfigSynchronizeError::Stopped,
        })
    }

    /// Drain only in-memory publications. No filesystem work occurs here.
    #[must_use]
    pub fn drain(&mut self) -> ConfigDrain {
        let drain = self.handle.inner.publications.drain_after(self.cursor);
        if let Some(latest) = drain.latest().cloned() {
            self.observe(&latest);
        }
        drain
    }

    /// Wait for an in-memory publication newer than this client's cursor.
    /// This is intended for composition and deterministic integration tests;
    /// renderer event/tick paths must remain on non-blocking [`Self::drain`].
    #[must_use]
    pub fn wait_for_drain(&mut self, timeout: Duration) -> ConfigDrain {
        self.handle
            .inner
            .publications
            .wait_for_change(self.cursor, timeout);
        self.drain()
    }

    /// Bounded composition-edge wait for the initial immutable publication.
    /// UI ticks should use [`Self::drain`] instead.
    #[must_use]
    pub fn wait_for_initial(&mut self, timeout: Duration) -> ConfigBootstrap {
        let (latest, timed_out) = self.handle.inner.publications.wait_for_latest(timeout);
        if let Some(latest) = latest {
            self.cursor = latest.id().get();
            self.observe(&latest);
            return ConfigBootstrap::Published(latest);
        }
        let stopped =
            *lock_unpoisoned(&self.handle.inner.lifecycle.state) == ConfigWorkerState::Stopped;
        ConfigBootstrap::Fallback {
            snapshot: Arc::new(Config::default()),
            source: if stopped && !timed_out {
                ConfigBootstrapFallback::WorkerStoppedDefault
            } else {
                ConfigBootstrapFallback::TimedOutDefault
            },
        }
    }

    fn observe(&mut self, publication: &ConfigPublication) {
        self.cursor = publication.id().get();
        self.observed = Some((publication.revision(), publication.snapshot().clone()));
    }
}

fn map_try_send_error<T>(error: TrySendError<T>) -> ConfigSubmitError {
    match error {
        TrySendError::Full(_) => ConfigSubmitError::Backpressure,
        TrySendError::Disconnected(_) => ConfigSubmitError::Stopped,
    }
}

fn config_worker(
    store: ConfigStore,
    commands: Receiver<ConfigCommand>,
    shutdown: Receiver<()>,
    publications: Arc<PublicationState>,
    lifecycle: Arc<WorkerLifecycle>,
    refresh_interval: Duration,
) {
    set_worker_state(&lifecycle, ConfigWorkerState::Running);
    let loaded = store.load_with_recovery();
    let mut current = Arc::new(loaded.config().clone());
    let mut revision = ConfigRevision::default().next();
    let mut last_recovery = ConfigRecovery::from(&loaded);
    publications.publish(
        revision,
        current.clone(),
        ConfigPublicationOutcome::Loaded(last_recovery),
    );

    let refresh = tick(refresh_interval);
    loop {
        select! {
            recv(shutdown) -> _ => break,
            recv(commands) -> command => {
                let Ok(command) = command else { break; };
                match command {
                    ConfigCommand::Submit { accepted_base, base, local } => {
                        match store.commit_patch(&base, &local) {
                            Ok(merged) => {
                                let changed = current.as_ref() != &merged;
                                if !changed {
                                    last_recovery = ConfigRecovery::primary();
                                    continue;
                                }
                                revision = revision.next();
                                current = Arc::new(merged);
                                last_recovery = ConfigRecovery::primary();
                                publications.publish(
                                    revision,
                                    current.clone(),
                                    ConfigPublicationOutcome::Saved { accepted_base },
                                );
                            }
                            Err(error) => publications.publish(
                                revision,
                                current.clone(),
                                ConfigPublicationOutcome::SaveFailed {
                                    accepted_base,
                                    error,
                                },
                            ),
                        }
                    }
                    ConfigCommand::Refresh => refresh_from_store(
                        &store,
                        &publications,
                        &mut current,
                        &mut revision,
                        &mut last_recovery,
                    ),
                    ConfigCommand::Barrier(ack) => {
                        let _ = ack.try_send(());
                    }
                }
            }
            recv(refresh) -> _ => refresh_from_store(
                &store,
                &publications,
                &mut current,
                &mut revision,
                &mut last_recovery,
            ),
        }
    }
    set_worker_state(&lifecycle, ConfigWorkerState::Stopped);
    publications.notify_all();
}

fn refresh_from_store(
    store: &ConfigStore,
    publications: &PublicationState,
    current: &mut Arc<Config>,
    revision: &mut ConfigRevision,
    last_recovery: &mut ConfigRecovery,
) {
    let loaded = store.load_with_recovery();
    let recovery = ConfigRecovery::from(&loaded);
    let recovery_changed = *last_recovery != recovery;
    if recovery.source() != ConfigLoadSource::Primary {
        if !recovery_changed {
            return;
        }
        *last_recovery = recovery;
        publications.publish(
            *revision,
            current.clone(),
            ConfigPublicationOutcome::RefreshFailed(recovery),
        );
        return;
    }
    let changed = current.as_ref() != loaded.config();
    if !changed && !recovery_changed {
        return;
    }
    if changed {
        *revision = revision.next();
        *current = Arc::new(loaded.config().clone());
    }
    *last_recovery = recovery;
    publications.publish(
        *revision,
        current.clone(),
        ConfigPublicationOutcome::Refreshed(recovery),
    );
}

fn set_worker_state(lifecycle: &WorkerLifecycle, next: ConfigWorkerState) {
    *lock_unpoisoned(&lifecycle.state) = next;
    lifecycle.changed.notify_all();
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
