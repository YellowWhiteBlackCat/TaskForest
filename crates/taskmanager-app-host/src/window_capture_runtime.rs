//! Process-wide bounded owner for current-window PNG capture.

use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, TrySendError, bounded, select_biased};
use taskmanager_application::window_capture::{
    WindowCaptureCompletion, WindowCaptureError, WindowCaptureErrorKind, WindowCaptureOutcome,
    WindowCapturePort, WindowCaptureRequest, WindowCaptureTarget,
};
use taskmanager_platform_contract::{WindowCaptureFailure, WindowCaptureReceipt};

use crate::worker_fault::catch_worker_panic;

pub const WINDOW_CAPTURE_COMMAND_CAPACITY: usize = 2;
const WINDOW_CAPTURE_COMPLETION_CAPACITY: usize = 2;
const WINDOW_CAPTURE_SHUTDOWN_WAIT: Duration = Duration::from_millis(100);
const MAX_WINDOW_CAPTURE_BYTES: u64 = 128 * 1024 * 1024;
static NEXT_WINDOW_CAPTURE_STAGE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowCaptureRuntimeStartError {
    detail: Arc<str>,
}

impl WindowCaptureRuntimeStartError {
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for WindowCaptureRuntimeStartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("window capture worker failed to start")
    }
}

struct CaptureCommand {
    request: WindowCaptureRequest,
    completion_tx: Sender<WindowCaptureCompletion>,
}

type CaptureExecutor =
    Arc<dyn Fn(&Path) -> Result<WindowCaptureReceipt, WindowCaptureFailure> + Send + Sync>;

struct WindowCaptureRuntimeInner {
    command_tx: Sender<CaptureCommand>,
    start_receivers: Mutex<Option<(Receiver<CaptureCommand>, Receiver<()>)>>,
    executor: CaptureExecutor,
    start_result: OnceLock<Result<(), Arc<str>>>,
    shutdown_tx: Sender<()>,
    done_tx: Sender<()>,
    done_rx: Receiver<()>,
    worker_exited: Arc<AtomicBool>,
    join: Mutex<Option<JoinHandle<()>>>,
}

impl WindowCaptureRuntimeInner {
    fn ensure_started(&self) -> Result<(), WindowCaptureRuntimeStartError> {
        let result = self.start_result.get_or_init(|| {
            let Some((command_rx, shutdown_rx)) = self
                .start_receivers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take()
            else {
                return Err(Arc::from("window capture worker start state was consumed"));
            };
            let executor = Arc::clone(&self.executor);
            let worker_exited = Arc::clone(&self.worker_exited);
            let done_tx = self.done_tx.clone();
            let join = std::thread::Builder::new()
                .name("taskforest-window-capture".to_owned())
                .stack_size(1024 * 1024)
                .spawn(move || {
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
            .map_err(|detail| WindowCaptureRuntimeStartError { detail })
    }
}

impl Drop for WindowCaptureRuntimeInner {
    fn drop(&mut self) {
        let join = self.join.get_mut().ok().and_then(Option::take);
        if let Some(join) = join {
            let _ = self.shutdown_tx.try_send(());
            if self
                .done_rx
                .recv_timeout(WINDOW_CAPTURE_SHUTDOWN_WAIT)
                .is_ok()
            {
                let _ = join.join();
            }
        }
    }
}

pub struct WindowCaptureCoordinator {
    inner: Arc<WindowCaptureRuntimeInner>,
}

impl fmt::Debug for WindowCaptureCoordinator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WindowCaptureCoordinator")
            .finish_non_exhaustive()
    }
}

impl WindowCaptureCoordinator {
    pub(crate) fn start() -> Result<Self, WindowCaptureRuntimeStartError> {
        Self::start_with_executor(Arc::new(|stage| {
            taskmanager_platform_native::capture_current_window_png(stage)
        }))
    }

    fn start_with_executor(
        executor: CaptureExecutor,
    ) -> Result<Self, WindowCaptureRuntimeStartError> {
        let (command_tx, command_rx) = bounded(WINDOW_CAPTURE_COMMAND_CAPACITY);
        let (shutdown_tx, shutdown_rx) = bounded(1);
        let (done_tx, done_rx) = bounded(1);
        Ok(Self {
            inner: Arc::new(WindowCaptureRuntimeInner {
                command_tx,
                start_receivers: Mutex::new(Some((command_rx, shutdown_rx))),
                executor,
                start_result: OnceLock::new(),
                shutdown_tx,
                done_tx,
                done_rx,
                worker_exited: Arc::new(AtomicBool::new(false)),
                join: Mutex::new(None),
            }),
        })
    }

    pub(crate) fn client(&self) -> WindowCaptureClient {
        let (completion_tx, completion_rx) = bounded(WINDOW_CAPTURE_COMPLETION_CAPACITY);
        WindowCaptureClient {
            inner: Arc::clone(&self.inner),
            completion_rx,
            completion_tx,
            outstanding: 0,
        }
    }
}

/// Named completion cursor for one frontend window.
pub struct WindowCaptureClient {
    inner: Arc<WindowCaptureRuntimeInner>,
    completion_rx: Receiver<WindowCaptureCompletion>,
    completion_tx: Sender<WindowCaptureCompletion>,
    outstanding: usize,
}

impl fmt::Debug for WindowCaptureClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WindowCaptureClient")
            .field("outstanding", &self.outstanding)
            .finish_non_exhaustive()
    }
}

impl WindowCaptureClient {
    pub fn try_submit(&mut self, request: WindowCaptureRequest) -> Result<(), WindowCaptureError> {
        <Self as WindowCapturePort>::try_submit(self, request)
    }

    pub fn drain(&mut self) -> Vec<WindowCaptureCompletion> {
        <Self as WindowCapturePort>::drain(self)
    }
}

impl WindowCapturePort for WindowCaptureClient {
    fn try_submit(&mut self, request: WindowCaptureRequest) -> Result<(), WindowCaptureError> {
        if self.inner.ensure_started().is_err() {
            return Err(runtime_error(
                WindowCaptureErrorKind::WorkerStopped,
                "window capture worker failed to start",
            ));
        }
        if self.inner.worker_exited.load(Ordering::Acquire) {
            self.outstanding = 0;
            return Err(runtime_error(
                WindowCaptureErrorKind::WorkerStopped,
                "window capture worker stopped",
            ));
        }
        if self.outstanding >= WINDOW_CAPTURE_COMPLETION_CAPACITY {
            return Err(runtime_error(
                WindowCaptureErrorKind::Backpressure,
                "window capture client completion lane is full",
            ));
        }
        let command = CaptureCommand {
            request,
            completion_tx: self.completion_tx.clone(),
        };
        match self.inner.command_tx.try_send(command) {
            Ok(()) => {
                self.outstanding = self.outstanding.saturating_add(1);
                Ok(())
            }
            Err(TrySendError::Full(_)) => Err(runtime_error(
                WindowCaptureErrorKind::Backpressure,
                "window capture request lane is full",
            )),
            Err(TrySendError::Disconnected(_)) => Err(runtime_error(
                WindowCaptureErrorKind::WorkerStopped,
                "window capture worker stopped",
            )),
        }
    }

    fn drain(&mut self) -> Vec<WindowCaptureCompletion> {
        let mut completions = Vec::new();
        while let Ok(completion) = self.completion_rx.try_recv() {
            self.outstanding = self.outstanding.saturating_sub(1);
            completions.push(completion);
        }
        if self.inner.worker_exited.load(Ordering::Acquire) && self.outstanding > 0 {
            self.outstanding = 0;
        }
        completions
    }
}

fn worker_loop(
    command_rx: Receiver<CaptureCommand>,
    shutdown_rx: Receiver<()>,
    executor: CaptureExecutor,
) {
    loop {
        select_biased! {
            recv(shutdown_rx) -> _ => return,
            recv(command_rx) -> command => {
                let Ok(command) = command else { return };
                if run_one_capture(&command, &executor) {
                    return;
                }
            }
        }
    }
}

fn run_one_capture(command: &CaptureCommand, executor: &CaptureExecutor) -> bool {
    let outcome = match catch_worker_panic(|| capture_request(&command.request, executor)) {
        Ok(outcome) => outcome,
        Err(detail) => WindowCaptureOutcome::Failed(runtime_error(
            WindowCaptureErrorKind::WorkerStopped,
            detail.as_ref(),
        )),
    };
    let faulted = matches!(outcome, WindowCaptureOutcome::Failed(ref error) if error.kind() == WindowCaptureErrorKind::WorkerStopped);
    let _ = command.completion_tx.try_send(WindowCaptureCompletion {
        request: command.request.id(),
        outcome,
    });
    faulted
}

fn capture_request(
    request: &WindowCaptureRequest,
    executor: &CaptureExecutor,
) -> WindowCaptureOutcome {
    let destination = match destination_path(request.target()) {
        Ok(path) => path,
        Err(error) => return WindowCaptureOutcome::Failed(error),
    };
    let stage = match create_stage_path(&destination) {
        Ok(path) => path,
        Err(error) => return WindowCaptureOutcome::Failed(error),
    };
    let mut stage = StageGuard::new(stage);
    let receipt = match executor(&stage.path) {
        Ok(receipt) => receipt,
        Err(error) => {
            return WindowCaptureOutcome::Failed(WindowCaptureError::new(
                WindowCaptureErrorKind::Native(error.kind()),
                error.detail(),
            ));
        }
    };
    if let Err(error) = validate_stage(&stage.path, receipt) {
        return WindowCaptureOutcome::Failed(error);
    }
    if let Err(error) = fs::rename(&stage.path, &destination) {
        return WindowCaptureOutcome::Failed(WindowCaptureError::new(
            WindowCaptureErrorKind::Commit,
            format!(
                "publish window screenshot {}: {error}",
                destination.display()
            ),
        ));
    }
    stage.committed = true;
    WindowCaptureOutcome::Ready {
        destination: Arc::from(destination.to_string_lossy().into_owned()),
        width: receipt.width(),
        height: receipt.height(),
        backend: receipt.backend(),
    }
}

struct StageGuard {
    path: PathBuf,
    committed: bool,
}

impl StageGuard {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            committed: false,
        }
    }
}

impl Drop for StageGuard {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn destination_path(target: &WindowCaptureTarget) -> Result<PathBuf, WindowCaptureError> {
    let destination = match target {
        WindowCaptureTarget::CurrentDirectory { filename } => {
            let filename_path = Path::new(filename.as_ref());
            if filename_path.components().count() != 1 || filename_path.file_name().is_none() {
                return Err(runtime_error(
                    WindowCaptureErrorKind::Inspect,
                    "window screenshot filename must be one path component",
                ));
            }
            std::env::current_dir()
                .map_err(|error| {
                    runtime_error(
                        WindowCaptureErrorKind::Inspect,
                        format!("resolve current screenshot directory: {error}"),
                    )
                })?
                .join(filename.as_ref())
        }
        WindowCaptureTarget::Path(path) => path.clone(),
    };
    if destination
        .extension()
        .and_then(|extension| extension.to_str())
        != Some("png")
    {
        return Err(runtime_error(
            WindowCaptureErrorKind::Inspect,
            format!(
                "window screenshot destination is not a .png path: {}",
                destination.display()
            ),
        ));
    }
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    match fs::symlink_metadata(parent) {
        Ok(metadata) if metadata.file_type().is_dir() => {}
        Ok(_) => {
            return Err(runtime_error(
                WindowCaptureErrorKind::Inspect,
                format!(
                    "window screenshot parent is not a directory: {}",
                    parent.display()
                ),
            ));
        }
        Err(error) => {
            return Err(runtime_error(
                WindowCaptureErrorKind::Inspect,
                format!(
                    "inspect window screenshot parent {}: {error}",
                    parent.display()
                ),
            ));
        }
    }
    match fs::symlink_metadata(&destination) {
        Ok(metadata) if metadata.file_type().is_file() => {}
        Ok(_) => {
            return Err(runtime_error(
                WindowCaptureErrorKind::Inspect,
                format!(
                    "refuse to replace non-regular screenshot path: {}",
                    destination.display()
                ),
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(runtime_error(
                WindowCaptureErrorKind::Inspect,
                format!(
                    "inspect window screenshot path {}: {error}",
                    destination.display()
                ),
            ));
        }
    }
    Ok(destination)
}

fn create_stage_path(destination: &Path) -> Result<PathBuf, WindowCaptureError> {
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let name = destination
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| "taskforest-window.png".into());
    for _ in 0..32 {
        let sequence = NEXT_WINDOW_CAPTURE_STAGE.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".{name}.{}-{sequence}.tmp", std::process::id()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(file) => {
                drop(file);
                return Ok(path);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(runtime_error(
                    WindowCaptureErrorKind::Stage,
                    format!(
                        "create window screenshot staging path {}: {error}",
                        path.display()
                    ),
                ));
            }
        }
    }
    Err(runtime_error(
        WindowCaptureErrorKind::Stage,
        "could not allocate window screenshot staging path",
    ))
}

fn validate_stage(stage: &Path, receipt: WindowCaptureReceipt) -> Result<(), WindowCaptureError> {
    let metadata = fs::symlink_metadata(stage).map_err(|error| {
        runtime_error(
            WindowCaptureErrorKind::Stage,
            format!("inspect captured window PNG: {error}"),
        )
    })?;
    if !metadata.file_type().is_file()
        || metadata.len() < 33
        || metadata.len() > MAX_WINDOW_CAPTURE_BYTES
    {
        return Err(runtime_error(
            WindowCaptureErrorKind::Stage,
            format!(
                "captured window PNG has an invalid size: {} bytes",
                metadata.len()
            ),
        ));
    }
    let mut file = fs::File::open(stage).map_err(|error| {
        runtime_error(
            WindowCaptureErrorKind::Stage,
            format!("open captured window PNG: {error}"),
        )
    })?;
    let mut header = [0_u8; 24];
    file.read_exact(&mut header).map_err(|error| {
        runtime_error(
            WindowCaptureErrorKind::Stage,
            format!("read captured window PNG header: {error}"),
        )
    })?;
    let ihdr_length = be_u32(&header[8..12]);
    let width = be_u32(&header[16..20]);
    let height = be_u32(&header[20..24]);
    if header[..8] != [137, 80, 78, 71, 13, 10, 26, 10]
        || &header[12..16] != b"IHDR"
        || ihdr_length != Some(13)
        || width != Some(receipt.width())
        || height != Some(receipt.height())
    {
        return Err(runtime_error(
            WindowCaptureErrorKind::Stage,
            "captured window PNG header does not match its native receipt",
        ));
    }
    Ok(())
}

fn be_u32(bytes: &[u8]) -> Option<u32> {
    <[u8; 4]>::try_from(bytes).ok().map(u32::from_be_bytes)
}

fn runtime_error(kind: WindowCaptureErrorKind, detail: impl Into<Arc<str>>) -> WindowCaptureError {
    WindowCaptureError::new(kind, detail)
}

#[cfg(test)]
#[path = "../tests/headless/window_capture_runtime_tests.rs"]
mod tests;
