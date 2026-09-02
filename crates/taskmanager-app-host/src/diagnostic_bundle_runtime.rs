//! Process-wide bounded owner for sanitized diagnostic file publication.

use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, TrySendError, bounded, select_biased};
use taskmanager_application::{
    DiagnosticBundleCompletion, DiagnosticBundlePort, DiagnosticBundleRequest,
    DiagnosticBundleTarget,
};
use taskmanager_core::{DiagnosticBundleError, DiagnosticBundleErrorKind, DiagnosticBundlePlan};

use crate::worker_fault::catch_worker_panic;

pub const DIAGNOSTIC_BUNDLE_COMMAND_CAPACITY: usize = 4;
const DIAGNOSTIC_BUNDLE_COMPLETION_CAPACITY: usize = 4;
const DIAGNOSTIC_BUNDLE_SHUTDOWN_WAIT: Duration = Duration::from_millis(100);
static NEXT_TRANSACTION: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticBundleRuntimeStartError {
    detail: Arc<str>,
}

impl DiagnosticBundleRuntimeStartError {
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for DiagnosticBundleRuntimeStartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("diagnostic bundle worker failed to start")
    }
}

struct BundleCommand {
    request: DiagnosticBundleRequest,
    completion_tx: Sender<DiagnosticBundleCompletion>,
}

struct DiagnosticBundleRuntimeInner {
    command_tx: Sender<BundleCommand>,
    start_receivers: Mutex<Option<(Receiver<BundleCommand>, Receiver<()>)>>,
    executor: DiagnosticBundleExecutor,
    start_result: OnceLock<Result<(), Arc<str>>>,
    shutdown_tx: Sender<()>,
    done_tx: Sender<()>,
    done_rx: Receiver<()>,
    /// Published with `Release` only after the worker's last completion send,
    /// so clients can prove no further completion can arrive.
    worker_exited: Arc<AtomicBool>,
    join: Mutex<Option<JoinHandle<()>>>,
}

impl DiagnosticBundleRuntimeInner {
    fn ensure_started(&self) -> Result<(), DiagnosticBundleRuntimeStartError> {
        let result = self.start_result.get_or_init(|| {
            let Some((command_rx, shutdown_rx)) = self
                .start_receivers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take()
            else {
                return Err(Arc::from(
                    "diagnostic bundle worker start state was consumed",
                ));
            };
            let executor = Arc::clone(&self.executor);
            let worker_exited = Arc::clone(&self.worker_exited);
            let done_tx = self.done_tx.clone();
            let join = std::thread::Builder::new()
                .name("taskforest-diagnostic-bundle".into())
                .stack_size(1024 * 1024)
                .spawn(move || {
                    // The thread-boundary catch is the exit-registration
                    // guarantee: a fault anywhere in the loop still marks the
                    // lane dead and publishes `done`.
                    let _ = catch_worker_panic(|| worker_loop(command_rx, shutdown_rx, executor));
                    worker_exited.store(true, Ordering::Release);
                    let _ = done_tx.try_send(());
                })
                .map_err(|error| Arc::<str>::from(error.to_string()));
            match join {
                Ok(join) => {
                    *self
                        .join
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(join);
                    Ok(())
                }
                Err(error) => Err(error),
            }
        });
        result
            .clone()
            .map_err(|detail| DiagnosticBundleRuntimeStartError { detail })
    }
}

impl Drop for DiagnosticBundleRuntimeInner {
    fn drop(&mut self) {
        let join = self.join.get_mut().ok().and_then(Option::take);
        if let Some(join) = join {
            let _ = self.shutdown_tx.try_send(());
            if self
                .done_rx
                .recv_timeout(DIAGNOSTIC_BUNDLE_SHUTDOWN_WAIT)
                .is_ok()
            {
                let _ = join.join();
            }
        }
    }
}

pub struct DiagnosticBundleCoordinator {
    inner: Arc<DiagnosticBundleRuntimeInner>,
}

/// One request's publication: the resolved destination plus its typed
/// outcome. The executor shape keeps filesystem work behind the same
/// composition seam the replay and export workers use for their loaders.
type DiagnosticBundleExecutor = Arc<
    dyn Fn(&DiagnosticBundleRequest) -> (PathBuf, Result<(), DiagnosticBundleError>) + Send + Sync,
>;

impl DiagnosticBundleCoordinator {
    pub(crate) fn start() -> Result<Self, DiagnosticBundleRuntimeStartError> {
        Self::start_with_executor(Arc::new(execute_bundle_request))
    }

    fn start_with_executor(
        execute: DiagnosticBundleExecutor,
    ) -> Result<Self, DiagnosticBundleRuntimeStartError> {
        let (command_tx, command_rx) = bounded(DIAGNOSTIC_BUNDLE_COMMAND_CAPACITY);
        let (shutdown_tx, shutdown_rx) = bounded(1);
        let (done_tx, done_rx) = bounded(1);
        let worker_exited = Arc::new(AtomicBool::new(false));
        Ok(Self {
            inner: Arc::new(DiagnosticBundleRuntimeInner {
                command_tx,
                start_receivers: Mutex::new(Some((command_rx, shutdown_rx))),
                executor: execute,
                start_result: OnceLock::new(),
                shutdown_tx,
                done_tx,
                done_rx,
                worker_exited,
                join: Mutex::new(None),
            }),
        })
    }

    #[must_use]
    pub fn client(&self) -> DiagnosticBundleClient {
        let (completion_tx, completion_rx) = bounded(DIAGNOSTIC_BUNDLE_COMPLETION_CAPACITY);
        DiagnosticBundleClient {
            inner: Arc::clone(&self.inner),
            completion_tx,
            completion_rx,
            outstanding: 0,
        }
    }
}

impl fmt::Debug for DiagnosticBundleCoordinator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DiagnosticBundleCoordinator")
            .finish_non_exhaustive()
    }
}

pub struct DiagnosticBundleClient {
    inner: Arc<DiagnosticBundleRuntimeInner>,
    completion_tx: Sender<DiagnosticBundleCompletion>,
    completion_rx: Receiver<DiagnosticBundleCompletion>,
    outstanding: usize,
}

impl fmt::Debug for DiagnosticBundleClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DiagnosticBundleClient")
            .field("outstanding", &self.outstanding)
            .finish_non_exhaustive()
    }
}

impl DiagnosticBundleClient {
    pub fn try_submit(
        &mut self,
        request: DiagnosticBundleRequest,
    ) -> Result<(), DiagnosticBundleError> {
        DiagnosticBundlePort::try_submit(self, request)
    }

    pub fn drain(&mut self) -> Vec<DiagnosticBundleCompletion> {
        DiagnosticBundlePort::drain(self)
    }
}

impl DiagnosticBundlePort for DiagnosticBundleClient {
    fn try_submit(
        &mut self,
        request: DiagnosticBundleRequest,
    ) -> Result<(), DiagnosticBundleError> {
        if self.inner.ensure_started().is_err() {
            return Err(DiagnosticBundleError::new(
                DiagnosticBundleErrorKind::Unavailable,
            ));
        }
        // A dead lane answers with its typed unavailability even while credits
        // stranded by the fault still occupy the completion budget; callers
        // must never mistake a gone worker for transient busyness.
        if self.inner.worker_exited.load(Ordering::Acquire) {
            return Err(DiagnosticBundleError::new(
                DiagnosticBundleErrorKind::Unavailable,
            ));
        }
        if self.outstanding >= DIAGNOSTIC_BUNDLE_COMPLETION_CAPACITY {
            return Err(DiagnosticBundleError::new(DiagnosticBundleErrorKind::Busy));
        }
        match self.inner.command_tx.try_send(BundleCommand {
            request,
            completion_tx: self.completion_tx.clone(),
        }) {
            Ok(()) => {
                self.outstanding = self.outstanding.saturating_add(1);
                Ok(())
            }
            Err(TrySendError::Full(_)) => {
                Err(DiagnosticBundleError::new(DiagnosticBundleErrorKind::Busy))
            }
            Err(TrySendError::Disconnected(_)) => Err(DiagnosticBundleError::new(
                DiagnosticBundleErrorKind::Unavailable,
            )),
        }
    }

    fn drain(&mut self) -> Vec<DiagnosticBundleCompletion> {
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

fn execute_bundle_request(
    request: &DiagnosticBundleRequest,
) -> (PathBuf, Result<(), DiagnosticBundleError>) {
    match resolve_target(request.target()) {
        Ok(destination) => {
            let result = write_plan_transactionally(request.plan(), &destination).map(|_| ());
            (destination, result)
        }
        Err(error) => (PathBuf::new(), Err(error)),
    }
}

fn worker_loop(
    command_rx: Receiver<BundleCommand>,
    shutdown_rx: Receiver<()>,
    execute: DiagnosticBundleExecutor,
) {
    loop {
        select_biased! {
            recv(shutdown_rx) -> _ => return,
            recv(command_rx) -> command => {
                let Ok(command) = command else { return };
                if run_one_bundle(&command, &execute) {
                    return;
                }
            }
        }
    }
}

/// Execute one publication with per-request isolation; returns `true` when
/// the executor faulted and the lane must exit.
///
/// Each publication is an independent encoded transaction over its own
/// request value, so the faulted request is resolved with a typed terminal
/// completion instead of stranding the submitter's credit. The worker still
/// exits after the first fault: a panic in this path proves an invariant
/// broke somewhere between redaction and rename, and an honestly dead lane
/// beats silently retrying the same broken code.
fn run_one_bundle(command: &BundleCommand, execute: &DiagnosticBundleExecutor) -> bool {
    let (destination, result, faulted) = match catch_worker_panic(|| execute(&command.request)) {
        Ok((destination, result)) => (destination, result, false),
        Err(detail) => (
            PathBuf::new(),
            Err(DiagnosticBundleError::with_detail(
                DiagnosticBundleErrorKind::Unavailable,
                detail.as_ref().to_owned(),
            )),
            true,
        ),
    };
    let _ = command.completion_tx.try_send(DiagnosticBundleCompletion {
        request: command.request.id(),
        destination,
        result,
    });
    faulted
}

fn resolve_target(target: &DiagnosticBundleTarget) -> Result<PathBuf, DiagnosticBundleError> {
    match target {
        DiagnosticBundleTarget::CurrentDirectory { file_name } => {
            if !target.is_valid() {
                return Err(DiagnosticBundleError::new(
                    DiagnosticBundleErrorKind::InvalidTarget,
                ));
            }
            std::env::current_dir()
                .map(|directory| directory.join(file_name))
                .map_err(|error| {
                    DiagnosticBundleError::with_detail(
                        DiagnosticBundleErrorKind::Io,
                        error.to_string(),
                    )
                })
        }
        DiagnosticBundleTarget::Path(path) => Ok(path.clone()),
    }
}

fn write_plan_transactionally(
    plan: &DiagnosticBundlePlan,
    destination: &Path,
) -> Result<PathBuf, DiagnosticBundleError> {
    let bytes = plan.encoded()?;
    transactional_write(destination, &bytes).map_err(|error| {
        DiagnosticBundleError::with_detail(DiagnosticBundleErrorKind::Io, error.to_string())
    })?;
    Ok(destination.to_path_buf())
}

fn transactional_write(destination: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let name = destination
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| "diagnostics.json".into());
    let mut temporary = None;
    for _ in 0..32 {
        let sequence = NEXT_TRANSACTION.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".{name}.{}-{sequence}.tmp", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
                    let _ = fs::remove_file(&path);
                    return Err(error);
                }
                temporary = Some(path);
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    let temporary = temporary.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate diagnostic staging file",
        )
    })?;
    if let Err(error) = fs::rename(&temporary, destination) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/headless/diagnostic_bundle_runtime_tests.rs"]
mod tests;
