//! Bounded fixed-argument child-process lifecycle shared by native adapters.
//!
//! This module owns spawning, pipe draining, memory ceilings, timeout and
//! cleanup only. It does not choose programs, parse output, classify provider
//! capabilities, or invoke a command interpreter on behalf of a caller.

use std::io::{self, Read};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

/// Maximum bytes retained from either stdout or stderr.
pub const MAX_CAPTURED_STREAM_BYTES: usize = 4 * 1024 * 1024;
/// Maximum bytes retained across stdout and stderr together.
pub const MAX_CAPTURED_TOTAL_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug)]
pub struct BoundedOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Debug)]
pub enum BoundedCommandError {
    Spawn(io::Error),
    ReaderStart(io::Error),
    ReaderFailed,
    ReaderTimedOut,
    ProcessTree,
    TimedOut,
    OutputTooLarge,
}

/// Owned process cleanup scope returned by a platform spawner.
///
/// The portable algorithm never sees native handles. Windows injects a strong
/// already-assigned kill-on-close Job; Unix supplies the fixed tool's process
/// group and relies on the documented non-daemonizing argv contract.
pub trait OwnedProcessTree {
    fn terminate(&mut self);
}

/// Child plus its already-established platform cleanup scope.
pub struct SpawnedCommand {
    child: Child,
    process_tree: Box<dyn OwnedProcessTree>,
}

impl SpawnedCommand {
    #[must_use]
    pub fn new(child: Child, process_tree: Box<dyn OwnedProcessTree>) -> Self {
        Self {
            child,
            process_tree,
        }
    }
}

/// Safe injection seam for platform-specific atomic spawn/ownership.
pub trait BoundedCommandSpawner {
    fn spawn(&self, command: &mut Command) -> Result<SpawnedCommand, BoundedCommandError>;
}

struct NativeCommandSpawner;

#[derive(Clone, Copy, Debug)]
enum DrainFailure {
    Read,
    Cancelled,
    OutputTooLarge,
}

impl From<DrainFailure> for BoundedCommandError {
    fn from(failure: DrainFailure) -> Self {
        match failure {
            DrainFailure::Read => Self::ReaderFailed,
            DrainFailure::Cancelled => Self::ReaderTimedOut,
            DrainFailure::OutputTooLarge => Self::OutputTooLarge,
        }
    }
}

/// Run one already-constructed fixed-argument command under shared bounds.
///
/// Callers own program/argv selection and output interpretation. This
/// function overwrites stdio with null stdin and piped stdout/stderr. Windows
/// adapters use [`run_with_spawner`] with their audited atomic Job spawner.
pub fn run_with_timeout(
    command: &mut Command,
    timeout: Duration,
) -> Result<BoundedOutput, BoundedCommandError> {
    run_with_spawner(command, timeout, &NativeCommandSpawner)
}

/// Run through a platform-owned atomic spawner while retaining the shared
/// timeout, pipe, output-limit and cleanup algorithm.
pub fn run_with_spawner(
    command: &mut Command,
    timeout: Duration,
    spawner: &dyn BoundedCommandSpawner,
) -> Result<BoundedOutput, BoundedCommandError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let SpawnedCommand {
        mut child,
        mut process_tree,
    } = spawner.spawn(command)?;
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            terminate_child(process_tree.as_mut(), &mut child);
            return Err(BoundedCommandError::ReaderFailed);
        }
    };
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            terminate_child(process_tree.as_mut(), &mut child);
            return Err(BoundedCommandError::ReaderFailed);
        }
    };

    let total = Arc::new(AtomicUsize::new(0));
    let cancelled = Arc::new(AtomicBool::new(false));
    let (failure_tx, failure_rx) = mpsc::sync_channel(2);
    let stdout_total = Arc::clone(&total);
    let stdout_cancelled = Arc::clone(&cancelled);
    let stdout_failure_tx = failure_tx.clone();
    let stdout_reader = match thread::Builder::new()
        .name("taskforest-command-stdout".to_owned())
        .spawn(move || drain(stdout, stdout_total, stdout_cancelled, stdout_failure_tx))
    {
        Ok(reader) => reader,
        Err(error) => {
            terminate_child(process_tree.as_mut(), &mut child);
            return Err(BoundedCommandError::ReaderStart(error));
        }
    };
    let stderr_reader = match thread::Builder::new()
        .name("taskforest-command-stderr".to_owned())
        .spawn({
            let cancelled = Arc::clone(&cancelled);
            move || drain(stderr, total, cancelled, failure_tx)
        }) {
        Ok(reader) => reader,
        Err(error) => {
            cancelled.store(true, Ordering::Release);
            terminate_child(process_tree.as_mut(), &mut child);
            let _ = stdout_reader.join();
            return Err(BoundedCommandError::ReaderStart(error));
        }
    };

    let deadline = Instant::now()
        .checked_add(timeout)
        .unwrap_or_else(Instant::now);
    let status = loop {
        if let Ok(failure) = failure_rx.try_recv() {
            cancelled.store(true, Ordering::Release);
            terminate_child(process_tree.as_mut(), &mut child);
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(failure.into());
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                process_tree.terminate();
                break status;
            }
            Ok(None) if Instant::now() >= deadline => {
                cancelled.store(true, Ordering::Release);
                terminate_child(process_tree.as_mut(), &mut child);
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(BoundedCommandError::TimedOut);
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(_) => {
                cancelled.store(true, Ordering::Release);
                terminate_child(process_tree.as_mut(), &mut child);
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(BoundedCommandError::ReaderFailed);
            }
        }
    };

    while !stdout_reader.is_finished() || !stderr_reader.is_finished() {
        if let Ok(failure) = failure_rx.try_recv() {
            cancelled.store(true, Ordering::Release);
            terminate_child(process_tree.as_mut(), &mut child);
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(failure.into());
        }
        if Instant::now() >= deadline {
            cancelled.store(true, Ordering::Release);
            terminate_child(process_tree.as_mut(), &mut child);
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(BoundedCommandError::ReaderTimedOut);
        }
        thread::sleep(Duration::from_millis(5));
    }

    // Join both before propagating either failure; no reader is detached.
    let stdout = join_reader(stdout_reader);
    let stderr = join_reader(stderr_reader);
    Ok(BoundedOutput {
        status,
        stdout: stdout?,
        stderr: stderr?,
    })
}

fn terminate_child(process_tree: &mut dyn OwnedProcessTree, child: &mut Child) {
    process_tree.terminate();
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
struct UnixProcessTree {
    process_group: i32,
}

#[cfg(unix)]
impl OwnedProcessTree for UnixProcessTree {
    fn terminate(&mut self) {
        let _ = nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(-self.process_group),
            nix::sys::signal::Signal::SIGKILL,
        );
    }
}

impl BoundedCommandSpawner for NativeCommandSpawner {
    fn spawn(&self, command: &mut Command) -> Result<SpawnedCommand, BoundedCommandError> {
        #[cfg(unix)]
        {
            command.process_group(0);
            let mut child = command.spawn().map_err(BoundedCommandError::Spawn)?;
            let process_group = match i32::try_from(child.id()) {
                Ok(process_group) => process_group,
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(BoundedCommandError::ProcessTree);
                }
            };
            Ok(SpawnedCommand::new(
                child,
                Box::new(UnixProcessTree { process_group }),
            ))
        }
        #[cfg(not(unix))]
        {
            let _ = command;
            // Windows must inject the audited suspended-Job spawner from the
            // native adapter. Failing closed prevents attach-after-spawn use.
            Err(BoundedCommandError::ProcessTree)
        }
    }
}

#[cfg(unix)]
fn drain(
    mut reader: impl Read + std::os::fd::AsFd,
    total: Arc<AtomicUsize>,
    cancelled: Arc<AtomicBool>,
    failure_tx: mpsc::SyncSender<DrainFailure>,
) -> Result<Vec<u8>, DrainFailure> {
    use nix::fcntl::{FcntlArg, OFlag, fcntl};

    let flags = fcntl(reader.as_fd(), FcntlArg::F_GETFL).map_err(|_| DrainFailure::Read)?;
    fcntl(
        reader.as_fd(),
        FcntlArg::F_SETFL(OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK),
    )
    .map_err(|_| DrainFailure::Read)?;
    drain_nonblocking(&mut reader, total, cancelled, failure_tx)
}

#[cfg(unix)]
fn drain_nonblocking(
    mut reader: impl Read,
    total: Arc<AtomicUsize>,
    cancelled: Arc<AtomicBool>,
    failure_tx: mpsc::SyncSender<DrainFailure>,
) -> Result<Vec<u8>, DrainFailure> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 8 * 1024];
    loop {
        if cancelled.load(Ordering::Acquire) {
            return Err(DrainFailure::Cancelled);
        }
        let read = match reader.read(&mut chunk) {
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(5));
                continue;
            }
            Err(_) => {
                let _ = failure_tx.send(DrainFailure::Read);
                return Err(DrainFailure::Read);
            }
        };
        if read == 0 {
            return Ok(buffer);
        }
        retain_chunk(&mut buffer, &chunk[..read], &total, &failure_tx)?;
    }
}

fn retain_chunk(
    buffer: &mut Vec<u8>,
    chunk: &[u8],
    total: &AtomicUsize,
    failure_tx: &mpsc::SyncSender<DrainFailure>,
) -> Result<(), DrainFailure> {
    if chunk.len() > MAX_CAPTURED_STREAM_BYTES.saturating_sub(buffer.len())
        || total
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current
                    .checked_add(chunk.len())
                    .filter(|next| *next <= MAX_CAPTURED_TOTAL_BYTES)
            })
            .is_err()
    {
        let _ = failure_tx.send(DrainFailure::OutputTooLarge);
        return Err(DrainFailure::OutputTooLarge);
    }
    buffer.try_reserve(chunk.len()).map_err(|_| {
        let _ = failure_tx.send(DrainFailure::OutputTooLarge);
        DrainFailure::OutputTooLarge
    })?;
    buffer.extend_from_slice(chunk);
    Ok(())
}

#[cfg(not(unix))]
fn drain(
    mut reader: impl Read,
    total: Arc<AtomicUsize>,
    cancelled: Arc<AtomicBool>,
    failure_tx: mpsc::SyncSender<DrainFailure>,
) -> Result<Vec<u8>, DrainFailure> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 8 * 1024];
    loop {
        if cancelled.load(Ordering::Acquire) {
            return Err(DrainFailure::Cancelled);
        }
        let read = reader.read(&mut chunk).map_err(|_| {
            let _ = failure_tx.send(DrainFailure::Read);
            DrainFailure::Read
        })?;
        if read == 0 {
            return Ok(buffer);
        }
        retain_chunk(&mut buffer, &chunk[..read], &total, &failure_tx)?;
    }
}

fn join_reader(
    handle: thread::JoinHandle<Result<Vec<u8>, DrainFailure>>,
) -> Result<Vec<u8>, BoundedCommandError> {
    handle
        .join()
        .map_err(|_| BoundedCommandError::ReaderFailed)?
        .map_err(Into::into)
}

#[cfg(test)]
#[path = "../tests/headless/command.rs"]
mod tests;
