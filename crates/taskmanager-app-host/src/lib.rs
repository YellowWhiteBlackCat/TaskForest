//! Toolkit-neutral native application composition.
//!
//! The three UI crates still own their event loops and renderer state, but
//! they all receive the same native composition seam from this crate.  This
//! keeps the platform axis (`cfg(target_os)` inside
//! `taskmanager-platform-native`) orthogonal to the UI axis (the selected
//! GPUI, Iced, or Ratatui feature).

#![forbid(unsafe_code)]

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use taskmanager_application::{
    ConfigClient, ConfigCoordinator, ConfigRuntimeStartError, PlatformClient,
};
use taskmanager_core::core::time::LocalTimeRulesObservation;
use taskmanager_platform_contract::{
    InstanceEvent, InstanceFailure, InstanceRole, TrayController, TrayFailure,
};
use taskmanager_platform_native::{
    NativePlatformRuntime, history_lock_holder_is_gone, native_config_path, native_history_dir,
    native_local_time_rules, native_locale_name,
};
use taskmanager_platform_runtime::CompositionError;

mod diagnostic_bundle_runtime;
mod history_frontend;
mod history_persistence_runtime;
mod history_replay_runtime;
mod presentation;
mod process_termination;
mod snapshot_export_runtime;
mod window_capture_runtime;
mod worker_fault;
use diagnostic_bundle_runtime::DiagnosticBundleCoordinator;
pub use diagnostic_bundle_runtime::{DiagnosticBundleClient, DiagnosticBundleRuntimeStartError};
pub use history_frontend::{
    HistoryFrontendConnectCompletion, HistoryFrontendConnectRequestId,
    HistoryFrontendConnectSubmitError, HistoryFrontendConnector,
    HistoryFrontendConnectorStartError, HistoryFrontendSession, HistoryFrontendStartError,
    HistoryFrontendStartErrorKind,
};
use history_persistence_runtime::HistoryPersistenceCoordinator;
pub use history_persistence_runtime::{
    HistoryPersistenceFailure, HistoryPersistenceFailureKind, HistoryPersistenceOperation,
    HistoryPersistenceRuntimeMonitor, HistoryPersistenceRuntimeStatus,
    HistoryPersistenceShutdownOutcome, HistoryPersistenceStartError,
    HistoryPersistenceStartErrorKind, HistoryPersistenceWorkerState, HistoryPersistenceWriter,
};
use history_replay_runtime::HistoryReplayCoordinator;
pub use history_replay_runtime::{HistoryReplayClient, HistoryReplayRuntimeStartError};
pub use presentation::{
    LayerShellAnchor, LayerShellFallbackPolicy, LayerShellKeyboardInteractivity, LayerShellLayer,
    LayerShellMargins, LayerShellOutput, LayerShellSize, LayerShellSpec, LayerShellSpecError,
    WindowPresentation,
};
pub use process_termination::{ProcessTermination, ProcessTerminationInstallError};
use snapshot_export_runtime::SnapshotExportCoordinator;
pub use snapshot_export_runtime::{SnapshotExportClient, SnapshotExportRuntimeStartError};
use window_capture_runtime::WindowCaptureCoordinator;
pub use window_capture_runtime::{WindowCaptureClient, WindowCaptureRuntimeStartError};

pub fn spawn_tray(
    spec: taskmanager_core::core::tray::TraySpec,
    events: std::sync::mpsc::Sender<taskmanager_core::core::tray::TrayEvent>,
) -> Result<Box<dyn TrayController>, TrayFailure> {
    taskmanager_platform_native::tray::spawn_tray(spec, events)
}

pub fn acquire_single_instance(
    instance_name: &str,
    events: std::sync::mpsc::Sender<InstanceEvent>,
) -> Result<InstanceRole, InstanceFailure> {
    taskmanager_platform_native::instance::acquire_single_instance(instance_name, events)
}

/// Invalidation policy for the host-owned local-time rules cache.
///
/// Runtime time-zone watching is intentionally not implied by a frontend
/// refresh. A new host process is the only event that re-reads native rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalTimeCacheInvalidation {
    HostRestartOnly,
}

struct StartupLocalTimeCache {
    observation: LocalTimeRulesObservation,
}

impl StartupLocalTimeCache {
    fn capture(observation: LocalTimeRulesObservation) -> Self {
        Self { observation }
    }

    fn observation(&self) -> LocalTimeRulesObservation {
        self.observation.clone()
    }

    const fn invalidation(&self) -> LocalTimeCacheInvalidation {
        LocalTimeCacheInvalidation::HostRestartOnly
    }
}

/// The shared native composition context passed to a selected UI launcher.
///
/// It contains only stable paths and exposes the platform factory as a method;
/// no toolkit types or renderer state cross this boundary.  The context is
/// cheap to clone so every window can receive an independent configuration
/// publication cursor backed by the same process-wide coordinator.
#[derive(Clone)]
pub struct NativeAppHost {
    config_path: PathBuf,
    history_root: PathBuf,
    local_time_cache: Arc<StartupLocalTimeCache>,
    config_runtime: Arc<OnceLock<Result<ConfigCoordinator, ConfigRuntimeStartError>>>,
    history_replay_runtime:
        Arc<OnceLock<Result<HistoryReplayCoordinator, HistoryReplayRuntimeStartError>>>,
    history_persistence_runtime:
        Arc<OnceLock<Result<HistoryPersistenceWriterGeneration, HistoryPersistenceStartError>>>,
    snapshot_export_runtime:
        Arc<OnceLock<Result<SnapshotExportCoordinator, SnapshotExportRuntimeStartError>>>,
    window_capture_runtime:
        Arc<OnceLock<Result<WindowCaptureCoordinator, WindowCaptureRuntimeStartError>>>,
    diagnostic_bundle_runtime:
        Arc<OnceLock<Result<DiagnosticBundleCoordinator, DiagnosticBundleRuntimeStartError>>>,
}

/// The host's single persistence-writer generation: one bounded start attempt,
/// handed to exactly one claimant.
///
/// [`HistoryPersistenceWriter`] is a move-only, shutdown-consuming capability,
/// so the cached generation holds the started writer behind a one-shot slot.
/// A caller that arrives after the slot was claimed re-enters the bounded
/// start path, which the history store's single-writer lock resolves as the
/// typed `Locked` failure while that generation is alive.
struct HistoryPersistenceWriterGeneration {
    writer: Mutex<Option<HistoryPersistenceWriter>>,
}

impl HistoryPersistenceWriterGeneration {
    fn claim(&self, root: &Path) -> Result<HistoryPersistenceWriter, HistoryPersistenceStartError> {
        let mut writer = self
            .writer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(writer) = writer.take() {
            return Ok(writer);
        }
        HistoryPersistenceCoordinator::start_path_bounded(root, history_lock_holder_is_gone)
            .map(HistoryPersistenceCoordinator::into_persistence_writer)
    }
}

impl fmt::Debug for NativeAppHost {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeAppHost")
            .field("config_path", &self.config_path)
            .field("history_root", &self.history_root)
            .field(
                "local_time_rules",
                &self.local_time_cache.observation.cache_key(),
            )
            .field(
                "local_time_cache_invalidation",
                &self.local_time_cache.invalidation(),
            )
            .field(
                "config_runtime_started",
                &self.config_runtime.get().is_some(),
            )
            .field(
                "history_replay_runtime_started",
                &self.history_replay_runtime.get().is_some(),
            )
            .field(
                "history_persistence_runtime_started",
                &self.history_persistence_runtime.get().is_some(),
            )
            .field(
                "snapshot_export_runtime_started",
                &self.snapshot_export_runtime.get().is_some(),
            )
            .field(
                "diagnostic_bundle_runtime_started",
                &self.diagnostic_bundle_runtime.get().is_some(),
            )
            .finish()
    }
}

impl PartialEq for NativeAppHost {
    fn eq(&self, other: &Self) -> bool {
        self.config_path == other.config_path && self.history_root == other.history_root
    }
}

impl Eq for NativeAppHost {}

impl NativeAppHost {
    /// Construct the production host using the selected native adapter's
    /// platform-specific user-data locations.
    #[must_use]
    pub fn production() -> Self {
        Self {
            config_path: native_config_path(),
            history_root: native_history_dir(),
            local_time_cache: Arc::new(StartupLocalTimeCache::capture(native_local_time_rules())),
            config_runtime: Arc::new(OnceLock::new()),
            history_replay_runtime: Arc::new(OnceLock::new()),
            history_persistence_runtime: Arc::new(OnceLock::new()),
            snapshot_export_runtime: Arc::new(OnceLock::new()),
            window_capture_runtime: Arc::new(OnceLock::new()),
            diagnostic_bundle_runtime: Arc::new(OnceLock::new()),
        }
    }

    /// Return the user configuration path selected by the native adapter.
    #[must_use]
    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    /// Create one window/client cursor backed by this host's shared lazy
    /// configuration coordinator. Filesystem work remains on the coordinator
    /// worker; callers only receive immutable publications and a bounded
    /// command seam.
    pub fn config_client(&self) -> Result<ConfigClient, ConfigRuntimeStartError> {
        self.config_runtime
            .get_or_init(|| ConfigCoordinator::start_path(&self.config_path))
            .as_ref()
            .map(ConfigCoordinator::client)
            .map_err(Clone::clone)
    }

    /// Create one bounded completion cursor backed by this process's single
    /// history-query worker. The client never exposes a filesystem path or a
    /// `HistoryQuery` to a frontend.
    fn enabled_history_replay_client(
        &self,
    ) -> Result<HistoryReplayClient, HistoryReplayRuntimeStartError> {
        self.history_replay_runtime
            .get_or_init(|| HistoryReplayCoordinator::start_path(&self.history_root))
            .as_ref()
            .map(HistoryReplayCoordinator::client)
            .map_err(Clone::clone)
    }

    /// Claim the persistent-history writer for this frontend process. Unlike
    /// the frontend session's replay client, this exposes only the write
    /// capability.
    ///
    /// The bounded start attempt is cached once per host exactly like the
    /// other lazy runtimes: a failed first attempt is reported unchanged to
    /// every later call on the same host, and one host hands its started
    /// generation to a single claimant instead of double-spawning a writer
    /// worker or lock attempt per repeated call. A cloned host shares the
    /// same generation and therefore cannot create a second writer.
    pub fn history_persistence_writer(
        &self,
    ) -> Result<HistoryPersistenceWriter, HistoryPersistenceStartError> {
        match self
            .history_persistence_runtime
            .get_or_init(|| {
                HistoryPersistenceCoordinator::start_path_bounded(
                    &self.history_root,
                    history_lock_holder_is_gone,
                )
                .map(|coordinator| HistoryPersistenceWriterGeneration {
                    writer: Mutex::new(Some(coordinator.into_persistence_writer())),
                })
            })
            .as_ref()
        {
            Ok(generation) => generation.claim(&self.history_root),
            Err(error) => Err(error.clone()),
        }
    }

    /// Create one named non-blocking client backed by the process-wide
    /// snapshot export worker. Serialization, current-directory discovery and
    /// transactional file publication remain behind this composition edge.
    pub fn snapshot_export_client(
        &self,
    ) -> Result<SnapshotExportClient, SnapshotExportRuntimeStartError> {
        self.snapshot_export_runtime
            .get_or_init(SnapshotExportCoordinator::start)
            .as_ref()
            .map(SnapshotExportCoordinator::client)
            .map_err(Clone::clone)
    }

    /// Create a named client backed by the process-wide current-window PNG
    /// capture worker. Native capture stays in the selected OS adapter; this
    /// host owns bounded request delivery and atomic filesystem publication.
    pub fn window_capture_client(
        &self,
    ) -> Result<WindowCaptureClient, WindowCaptureRuntimeStartError> {
        self.window_capture_runtime
            .get_or_init(WindowCaptureCoordinator::start)
            .as_ref()
            .map(WindowCaptureCoordinator::client)
            .map_err(Clone::clone)
    }

    /// Create a named client backed by the process-wide diagnostic publication
    /// worker. Multiple windows/features receive independent completion lanes,
    /// while thread and filesystem ownership remain unique in the host.
    pub fn diagnostic_bundle_client(
        &self,
    ) -> Result<DiagnosticBundleClient, DiagnosticBundleRuntimeStartError> {
        self.diagnostic_bundle_runtime
            .get_or_init(DiagnosticBundleCoordinator::start)
            .as_ref()
            .map(DiagnosticBundleCoordinator::client)
            .map_err(Clone::clone)
    }

    /// Return the native locale hint used only when no saved language
    /// preference exists. The UI never reads an OS registry or API directly.
    #[must_use]
    pub fn native_locale_name(&self) -> Option<String> {
        native_locale_name()
    }

    /// Return the immutable native local-time rule observation. Discovery and
    /// validation stay in the selected OS adapter; no filesystem path or
    /// host-reading callback crosses into a frontend.
    #[must_use]
    pub fn local_time_rules(&self) -> LocalTimeRulesObservation {
        self.local_time_cache.observation()
    }

    /// Return the cache invalidation contract shared by every cloned host and
    /// window. A frontend refresh never triggers native time-zone discovery.
    #[must_use]
    pub fn local_time_cache_invalidation(&self) -> LocalTimeCacheInvalidation {
        self.local_time_cache.invalidation()
    }

    /// Spawn the selected OS adapter and wrap it in the application port
    /// consumed by a frontend or the neutral CLI.
    pub fn spawn_client(&self) -> Result<PlatformClient, CompositionError> {
        NativePlatformRuntime::spawn().map(PlatformClient::new)
    }
}
