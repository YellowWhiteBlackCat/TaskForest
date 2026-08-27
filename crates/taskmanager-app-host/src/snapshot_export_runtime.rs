//! Bounded background owner for snapshot serialization and filesystem I/O.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, TrySendError, bounded, select_biased};
use taskmanager_application::snapshot_export::{
    SnapshotExportCompletion, SnapshotExportError, SnapshotExportErrorKind, SnapshotExportOutcome,
    SnapshotExportPort, SnapshotExportRequest, SnapshotExportTarget,
};
use taskmanager_core::core::export::{processes_to_csv, processes_to_html, snapshot_to_json};

use crate::worker_fault::catch_worker_panic;

pub const SNAPSHOT_EXPORT_COMMAND_CAPACITY: usize = 4;
const SNAPSHOT_EXPORT_COMPLETION_CAPACITY: usize = 4;
const SNAPSHOT_EXPORT_SHUTDOWN_WAIT: Duration = Duration::from_millis(100);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotExportRuntimeStartError {
    detail: Arc<str>,
}

impl SnapshotExportRuntimeStartError {
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for SnapshotExportRuntimeStartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("snapshot export worker failed to start")
    }
}

struct ExportCommand {
    request: SnapshotExportRequest,
    completion_tx: Sender<SnapshotExportCompletion>,
}

struct SnapshotExportRuntimeInner {
    command_tx: Sender<ExportCommand>,
    shutdown_tx: Sender<()>,
    done_rx: Receiver<()>,
    /// Published with `Release` only after the worker's last completion send,
    /// so clients can prove no further completion can arrive.
    worker_exited: Arc<AtomicBool>,
    join: Mutex<Option<JoinHandle<()>>>,
}

impl Drop for SnapshotExportRuntimeInner {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.try_send(());
        let join = self.join.get_mut().ok().and_then(Option::take);
        if self
            .done_rx
            .recv_timeout(SNAPSHOT_EXPORT_SHUTDOWN_WAIT)
            .is_ok()
            && let Some(join) = join
        {
            let _ = join.join();
        }
        // A hostile filesystem can fail to return. The bounded shutdown wait
        // keeps window teardown finite; a detached completion has no frontend
        // authority after its client/controller is dropped.
    }
}

pub struct SnapshotExportCoordinator {
    inner: Arc<SnapshotExportRuntimeInner>,
}

impl fmt::Debug for SnapshotExportCoordinator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SnapshotExportCoordinator")
            .finish_non_exhaustive()
    }
}

impl SnapshotExportCoordinator {
    pub(crate) fn start() -> Result<Self, SnapshotExportRuntimeStartError> {
        Self::start_with_exporter(Arc::new(export_request))
    }

    fn start_with_exporter(
        exporter: Arc<dyn Fn(&SnapshotExportRequest) -> SnapshotExportOutcome + Send + Sync>,
    ) -> Result<Self, SnapshotExportRuntimeStartError> {
        let (command_tx, command_rx) = bounded(SNAPSHOT_EXPORT_COMMAND_CAPACITY);
        let (shutdown_tx, shutdown_rx) = bounded(1);
        let (done_tx, done_rx) = bounded(1);
        let worker_exited = Arc::new(AtomicBool::new(false));
        let worker_exited_for_thread = Arc::clone(&worker_exited);
        let join = std::thread::Builder::new()
            .name("taskforest-snapshot-export".to_owned())
            .spawn(move || {
                // The thread-boundary catch is the exit-registration
                // guarantee: a fault anywhere in the loop still marks the
                // lane dead and publishes `done`, so the bounded shutdown
                // seam never waits on a thread that died mid-unwind.
                let _ = catch_worker_panic(|| worker_loop(command_rx, shutdown_rx, exporter));
                worker_exited_for_thread.store(true, Ordering::Release);
                let _ = done_tx.try_send(());
            })
            .map_err(|error| SnapshotExportRuntimeStartError {
                detail: Arc::from(error.to_string()),
            })?;
        Ok(Self {
            inner: Arc::new(SnapshotExportRuntimeInner {
                command_tx,
                shutdown_tx,
                done_rx,
                worker_exited,
                join: Mutex::new(Some(join)),
            }),
        })
    }

    #[must_use]
    pub fn client(&self) -> SnapshotExportClient {
        let (completion_tx, completion_rx) = bounded(SNAPSHOT_EXPORT_COMPLETION_CAPACITY);
        SnapshotExportClient {
            inner: Arc::clone(&self.inner),
            completion_tx,
            completion_rx,
            outstanding: 0,
        }
    }
}

pub struct SnapshotExportClient {
    inner: Arc<SnapshotExportRuntimeInner>,
    completion_tx: Sender<SnapshotExportCompletion>,
    completion_rx: Receiver<SnapshotExportCompletion>,
    outstanding: usize,
}

impl fmt::Debug for SnapshotExportClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SnapshotExportClient")
            .field("outstanding", &self.outstanding)
            .finish_non_exhaustive()
    }
}

impl SnapshotExportClient {
    pub fn try_submit(
        &mut self,
        request: SnapshotExportRequest,
    ) -> Result<(), SnapshotExportError> {
        SnapshotExportPort::try_submit(self, request)
    }

    pub fn drain(&mut self) -> Vec<SnapshotExportCompletion> {
        SnapshotExportPort::drain(self)
    }
}

impl SnapshotExportPort for SnapshotExportClient {
    fn try_submit(&mut self, request: SnapshotExportRequest) -> Result<(), SnapshotExportError> {
        // A dead lane answers with its typed stop even while credits stranded
        // by the fault still occupy the completion budget; callers must never
        // mistake a gone worker for transient backpressure.
        if self.inner.worker_exited.load(Ordering::Acquire) {
            return Err(runtime_error(
                SnapshotExportErrorKind::WorkerStopped,
                "snapshot export worker stopped",
            ));
        }
        if self.outstanding >= SNAPSHOT_EXPORT_COMPLETION_CAPACITY {
            return Err(runtime_error(
                SnapshotExportErrorKind::Backpressure,
                "snapshot export client completion lane is full",
            ));
        }
        match self.inner.command_tx.try_send(ExportCommand {
            request,
            completion_tx: self.completion_tx.clone(),
        }) {
            Ok(()) => {
                self.outstanding = self.outstanding.saturating_add(1);
                Ok(())
            }
            Err(TrySendError::Full(_)) => Err(runtime_error(
                SnapshotExportErrorKind::Backpressure,
                "snapshot export command lane is full",
            )),
            Err(TrySendError::Disconnected(_)) => Err(runtime_error(
                SnapshotExportErrorKind::WorkerStopped,
                "snapshot export worker stopped",
            )),
        }
    }

    fn drain(&mut self) -> Vec<SnapshotExportCompletion> {
        let mut completions = Vec::with_capacity(self.outstanding);
        while let Ok(completion) = self.completion_rx.try_recv() {
            self.outstanding = self.outstanding.saturating_sub(1);
            completions.push(completion);
        }
        if self.inner.worker_exited.load(Ordering::Acquire) {
            // The exit flag is published only after the worker's final
            // completion send, so an empty lane plus this flag proves no
            // further completion can arrive. Every remaining credit belonged
            // to a request stranded by the fault and is released here instead
            // of permanently consuming the client's admission bound.
            self.outstanding = 0;
        }
        completions
    }
}

fn worker_loop(
    command_rx: Receiver<ExportCommand>,
    shutdown_rx: Receiver<()>,
    exporter: Arc<dyn Fn(&SnapshotExportRequest) -> SnapshotExportOutcome + Send + Sync>,
) {
    loop {
        select_biased! {
            recv(shutdown_rx) -> _ => return,
            recv(command_rx) -> command => {
                let Ok(command) = command else { return };
                if run_one_export(&command, &exporter) {
                    return;
                }
            }
        }
    }
}

/// Execute one export with per-request isolation; returns `true` when the
/// exporter faulted and the lane must exit.
///
/// Each export is an independent serialization plus transaction over its own
/// request value, so the faulted request is resolved with a typed terminal
/// completion instead of stranding the submitter's credit. The worker still
/// exits after the first fault: the serializers and filesystem transaction
/// share process-wide statics and OS state a panic may have disturbed, and an
/// honestly dead lane beats silently retrying over the same fault.
fn run_one_export(
    command: &ExportCommand,
    exporter: &Arc<dyn Fn(&SnapshotExportRequest) -> SnapshotExportOutcome + Send + Sync>,
) -> bool {
    let (outcome, faulted) = match catch_worker_panic(|| exporter(&command.request)) {
        Ok(outcome) => (outcome, false),
        Err(detail) => (
            SnapshotExportOutcome::Failed(runtime_error(
                SnapshotExportErrorKind::WorkerStopped,
                &detail,
            )),
            true,
        ),
    };
    let _ = command.completion_tx.try_send(SnapshotExportCompletion {
        request: command.request.id(),
        outcome,
    });
    faulted
}

fn export_request(request: &SnapshotExportRequest) -> SnapshotExportOutcome {
    match write_snapshot(request) {
        Ok(base) => SnapshotExportOutcome::Ready {
            base: Arc::from(base.to_string_lossy().into_owned()),
        },
        Err(error) => SnapshotExportOutcome::Failed(error),
    }
}

fn write_snapshot(request: &SnapshotExportRequest) -> Result<PathBuf, SnapshotExportError> {
    let payload = request.payload();
    let base = match payload.target() {
        SnapshotExportTarget::CurrentDirectory { stem } => std::env::current_dir()
            .map_err(|error| {
                runtime_error(
                    SnapshotExportErrorKind::Inspect,
                    &format!("resolve current export directory: {error}"),
                )
            })?
            .join(stem.as_ref()),
        SnapshotExportTarget::BasePath(base) => base.clone(),
    };
    let json = snapshot_to_json(payload.snapshot(), payload.processes());
    let csv = processes_to_csv(payload.processes());
    let html = processes_to_html(payload.snapshot(), payload.processes());
    let mut transaction = ExportTransaction::new(&RealFileSystem, &base)?;
    transaction.stage([&json, &csv, &html])?;
    transaction.commit()?;
    Ok(base)
}

trait ExportFileSystem {
    fn create_dir(&self, path: &Path) -> io::Result<()>;
    fn symlink_metadata(&self, path: &Path) -> io::Result<fs::Metadata>;
    fn write(&self, path: &Path, contents: &[u8]) -> io::Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    fn remove_dir(&self, path: &Path) -> io::Result<()>;
}

struct RealFileSystem;

impl ExportFileSystem for RealFileSystem {
    fn create_dir(&self, path: &Path) -> io::Result<()> {
        fs::create_dir(path)
    }
    fn symlink_metadata(&self, path: &Path) -> io::Result<fs::Metadata> {
        fs::symlink_metadata(path)
    }
    fn write(&self, path: &Path, contents: &[u8]) -> io::Result<()> {
        fs::write(path, contents)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }
    fn remove_dir(&self, path: &Path) -> io::Result<()> {
        fs::remove_dir(path)
    }
}

static NEXT_EXPORT_TRANSACTION: AtomicU64 = AtomicU64::new(0);

fn create_transaction_dir(
    file_system: &impl ExportFileSystem,
    base: &Path,
) -> Result<PathBuf, SnapshotExportError> {
    let parent = base.parent().unwrap_or_else(|| Path::new("."));
    let base_name = base
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| "snapshot".into());
    loop {
        let sequence = NEXT_EXPORT_TRANSACTION.fetch_add(1, Ordering::Relaxed);
        let name = format!(".{base_name}.export-{}-{sequence}.tmp", std::process::id());
        let transaction_dir = parent.join(name);
        match file_system.create_dir(&transaction_dir) {
            Ok(()) => return Ok(transaction_dir),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(runtime_error(
                    SnapshotExportErrorKind::Stage,
                    &format!(
                        "create export staging directory {}: {error}",
                        transaction_dir.display()
                    ),
                ));
            }
        }
    }
}

struct ExportArtifact {
    final_path: PathBuf,
    staged_path: PathBuf,
    backup_path: PathBuf,
    backup_moved: bool,
    installed: bool,
}

struct ExportTransaction<'a, F: ExportFileSystem> {
    file_system: &'a F,
    directory: PathBuf,
    artifacts: [ExportArtifact; 3],
}

impl<'a, F: ExportFileSystem> ExportTransaction<'a, F> {
    fn new(file_system: &'a F, base: &Path) -> Result<Self, SnapshotExportError> {
        for final_path in ["json", "csv", "html"].map(|extension| base.with_extension(extension)) {
            match file_system.symlink_metadata(&final_path) {
                Ok(metadata) if metadata.file_type().is_file() => {}
                Ok(_) => {
                    return Err(runtime_error(
                        SnapshotExportErrorKind::Inspect,
                        &format!(
                            "refuse to replace non-regular export path {}",
                            final_path.display()
                        ),
                    ));
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(runtime_error(
                        SnapshotExportErrorKind::Inspect,
                        &format!("inspect export path {}: {error}", final_path.display()),
                    ));
                }
            }
        }
        let directory = create_transaction_dir(file_system, base)?;
        let artifacts = ["json", "csv", "html"].map(|extension| ExportArtifact {
            final_path: base.with_extension(extension),
            staged_path: directory.join(format!("snapshot.{extension}.tmp")),
            backup_path: directory.join(format!("snapshot.{extension}.bak")),
            backup_moved: false,
            installed: false,
        });
        Ok(Self {
            file_system,
            directory,
            artifacts,
        })
    }

    fn stage(&self, contents: [&str; 3]) -> Result<(), SnapshotExportError> {
        for (artifact, contents) in self.artifacts.iter().zip(contents) {
            if let Err(error) = self
                .file_system
                .write(&artifact.staged_path, contents.as_bytes())
            {
                let failure = runtime_error(
                    SnapshotExportErrorKind::Stage,
                    &format!("write {}: {error}", artifact.staged_path.display()),
                );
                return Err(self.clean_staging_after(failure));
            }
        }
        Ok(())
    }

    fn clean_staging_after(&self, failure: SnapshotExportError) -> SnapshotExportError {
        let cleanup = self.clean_disposable(false);
        if cleanup.is_empty() {
            failure
        } else {
            runtime_error(
                failure.kind(),
                &format!("{}; cleanup: {}", failure.detail(), cleanup.join("; ")),
            )
        }
    }

    fn clean_disposable(&self, include_backups: bool) -> Vec<String> {
        let mut errors = Vec::new();
        for artifact in &self.artifacts {
            for path in [
                Some(&artifact.staged_path),
                include_backups.then_some(&artifact.backup_path),
            ]
            .into_iter()
            .flatten()
            {
                match self.file_system.remove_file(path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => errors.push(format!("remove {}: {error}", path.display())),
                }
            }
        }
        match self.file_system.remove_dir(&self.directory) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => errors.push(format!("remove {}: {error}", self.directory.display())),
        }
        errors
    }

    fn commit(&mut self) -> Result<(), SnapshotExportError> {
        for artifact in &mut self.artifacts {
            match self
                .file_system
                .rename(&artifact.final_path, &artifact.backup_path)
            {
                Ok(()) => artifact.backup_moved = true,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => {
                    let failure = runtime_error(
                        SnapshotExportErrorKind::Commit,
                        &format!("back up {}: {error}", artifact.final_path.display()),
                    );
                    return Err(self.rollback_after(failure));
                }
            }
            if let Err(error) = self
                .file_system
                .rename(&artifact.staged_path, &artifact.final_path)
            {
                let failure = runtime_error(
                    SnapshotExportErrorKind::Commit,
                    &format!("commit {}: {error}", artifact.final_path.display()),
                );
                return Err(self.rollback_after(failure));
            }
            artifact.installed = true;
        }
        let _ = self.clean_disposable(true);
        Ok(())
    }

    fn rollback_after(&mut self, failure: SnapshotExportError) -> SnapshotExportError {
        let mut errors = Vec::new();
        for artifact in self.artifacts.iter_mut().rev() {
            let mut final_clear = !artifact.installed;
            if artifact.installed {
                match self.file_system.remove_file(&artifact.final_path) {
                    Ok(()) => final_clear = true,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => final_clear = true,
                    Err(error) => errors.push(format!(
                        "remove partial {}: {error}",
                        artifact.final_path.display()
                    )),
                }
                artifact.installed = false;
            }
            if artifact.backup_moved && final_clear {
                match self
                    .file_system
                    .rename(&artifact.backup_path, &artifact.final_path)
                {
                    Ok(()) => artifact.backup_moved = false,
                    Err(error) => errors.push(format!(
                        "restore backup {}: {error}",
                        artifact.final_path.display()
                    )),
                }
            }
        }
        if self.artifacts.iter().any(|artifact| artifact.backup_moved) {
            errors.push(format!("backup retained in {}", self.directory.display()));
        } else {
            errors.extend(self.clean_disposable(true));
        }
        if errors.is_empty() {
            failure
        } else {
            runtime_error(
                SnapshotExportErrorKind::Rollback,
                &format!("{}; rollback: {}", failure.detail(), errors.join("; ")),
            )
        }
    }
}

fn runtime_error(kind: SnapshotExportErrorKind, detail: &str) -> SnapshotExportError {
    SnapshotExportError::new(kind, Arc::<str>::from(detail))
}

#[cfg(test)]
#[path = "../tests/headless/snapshot_export_runtime_tests.rs"]
mod tests;
