//! Bounded background owner for persistent-history query I/O.

use std::fmt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossbeam_channel::{Receiver, Sender, TrySendError, bounded, select_biased};
use taskmanager_application::{
    HistoryReplayCompletion, HistoryReplayCompletionOutcome, HistoryReplayError,
    HistoryReplayErrorKind, HistoryReplayRequest, HistoryReplayRow, MAX_HISTORY_REPLAY_POINTS,
};
use taskmanager_history_store::{HistoryQuery, HistoryStoreError, HistoryStoreErrorKind};

use crate::worker_fault::catch_worker_panic;

pub const HISTORY_REPLAY_COMMAND_CAPACITY: usize = 4;
const HISTORY_REPLAY_COMPLETION_CAPACITY: usize = 4;
const HISTORY_REPLAY_SHUTDOWN_WAIT: Duration = Duration::from_millis(100);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryReplayRuntimeStartError {
    detail: Arc<str>,
}

impl HistoryReplayRuntimeStartError {
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for HistoryReplayRuntimeStartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("history replay worker failed to start")
    }
}

struct QueryCommand {
    request: HistoryReplayRequest,
    completion_tx: Sender<HistoryReplayCompletion>,
}

struct HistoryReplayRuntimeInner {
    command_tx: Sender<QueryCommand>,
    shutdown_tx: Sender<()>,
    done_rx: Receiver<()>,
    /// Published with `Release` only after the worker's last completion send,
    /// so clients can prove no further completion can arrive.
    worker_exited: Arc<AtomicBool>,
    join: Mutex<Option<JoinHandle<()>>>,
}

impl Drop for HistoryReplayRuntimeInner {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.try_send(());
        let join = self.join.get_mut().ok().and_then(Option::take);
        if self
            .done_rx
            .recv_timeout(HISTORY_REPLAY_SHUTDOWN_WAIT)
            .is_ok()
        {
            if let Some(join) = join {
                let _ = join.join();
            }
        } else {
            // Read sizes and directory cardinality are bounded, but an OS
            // filesystem can still fail to return. Detaching a read-only
            // worker is safer than freezing the final window/host drop; its
            // disconnected completion has no authority to mutate UI state.
            drop(join);
        }
    }
}

pub struct HistoryReplayCoordinator {
    inner: Arc<HistoryReplayRuntimeInner>,
}

impl fmt::Debug for HistoryReplayCoordinator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HistoryReplayCoordinator")
            .finish_non_exhaustive()
    }
}

impl HistoryReplayCoordinator {
    pub(crate) fn start_path(
        path: impl AsRef<Path>,
    ) -> Result<Self, HistoryReplayRuntimeStartError> {
        let query = HistoryQuery::new(path.as_ref());
        Self::start_with_loader(Arc::new(move |request, now_ms| {
            query_rows(&query, request, now_ms)
        }))
    }

    fn start_with_loader(
        loader: HistoryReplayLoader,
    ) -> Result<Self, HistoryReplayRuntimeStartError> {
        let (command_tx, command_rx) = bounded(HISTORY_REPLAY_COMMAND_CAPACITY);
        let (shutdown_tx, shutdown_rx) = bounded(1);
        let (done_tx, done_rx) = bounded(1);
        let worker_exited = Arc::new(AtomicBool::new(false));
        let worker_exited_for_thread = Arc::clone(&worker_exited);
        let join = std::thread::Builder::new()
            .name("taskforest-history-replay".to_owned())
            .spawn(move || {
                // The thread-boundary catch is the exit-registration
                // guarantee: even a fault outside the per-request seam must
                // still mark the lane dead and publish `done`, keeping the
                // bounded shutdown seam honest instead of stranding it on a
                // thread that died mid-unwind.
                let _ = catch_worker_panic(|| worker_loop(command_rx, shutdown_rx, loader));
                worker_exited_for_thread.store(true, Ordering::Release);
                let _ = done_tx.try_send(());
            })
            .map_err(|error| HistoryReplayRuntimeStartError {
                detail: Arc::from(error.to_string()),
            })?;
        Ok(Self {
            inner: Arc::new(HistoryReplayRuntimeInner {
                command_tx,
                shutdown_tx,
                done_rx,
                worker_exited,
                join: Mutex::new(Some(join)),
            }),
        })
    }

    #[must_use]
    pub fn client(&self) -> HistoryReplayClient {
        let (completion_tx, completion_rx) = bounded(HISTORY_REPLAY_COMPLETION_CAPACITY);
        HistoryReplayClient {
            inner: Arc::clone(&self.inner),
            completion_tx,
            completion_rx,
            outstanding: 0,
        }
    }
}

pub struct HistoryReplayClient {
    inner: Arc<HistoryReplayRuntimeInner>,
    completion_tx: Sender<HistoryReplayCompletion>,
    completion_rx: Receiver<HistoryReplayCompletion>,
    outstanding: usize,
}

impl fmt::Debug for HistoryReplayClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HistoryReplayClient")
            .field("outstanding", &self.outstanding)
            .finish_non_exhaustive()
    }
}

impl HistoryReplayClient {
    pub fn try_request(&mut self, request: HistoryReplayRequest) -> Result<(), HistoryReplayError> {
        // A dead lane answers with its typed stop even while credits stranded
        // by the fault still occupy the completion budget; otherwise callers
        // cannot distinguish "busy" from "gone" and would retry forever.
        if self.inner.worker_exited.load(Ordering::Acquire) {
            return Err(runtime_error(
                HistoryReplayErrorKind::WorkerStopped,
                "history replay worker stopped",
            ));
        }
        if self.outstanding >= HISTORY_REPLAY_COMPLETION_CAPACITY {
            return Err(runtime_error(
                HistoryReplayErrorKind::Backpressure,
                "client completion lane is full",
            ));
        }
        match self.inner.command_tx.try_send(QueryCommand {
            request,
            completion_tx: self.completion_tx.clone(),
        }) {
            Ok(()) => {
                self.outstanding = self.outstanding.saturating_add(1);
                Ok(())
            }
            Err(TrySendError::Full(_)) => Err(runtime_error(
                HistoryReplayErrorKind::Backpressure,
                "history replay command lane is full",
            )),
            Err(TrySendError::Disconnected(_)) => Err(runtime_error(
                HistoryReplayErrorKind::WorkerStopped,
                "history replay worker stopped",
            )),
        }
    }

    pub fn drain(&mut self) -> Vec<HistoryReplayCompletion> {
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

type HistoryReplayLoader = Arc<
    dyn Fn(HistoryReplayRequest, u64) -> Result<Arc<[HistoryReplayRow]>, HistoryReplayError>
        + Send
        + Sync,
>;

fn worker_loop(
    command_rx: Receiver<QueryCommand>,
    shutdown_rx: Receiver<()>,
    loader: HistoryReplayLoader,
) {
    loop {
        select_biased! {
            recv(shutdown_rx) -> _ => return,
            recv(command_rx) -> command => {
                let Ok(command) = command else { return };
                if run_one_query(&command, &loader) {
                    return;
                }
            }
        }
    }
}

/// Execute one query with per-request isolation; returns `true` when the
/// loader faulted and the lane must exit.
///
/// Reads through an immutable `HistoryQuery` are stateless for this worker,
/// so the faulted request itself is resolved with a typed terminal
/// completion instead of stranding the submitter's credit forever. The loop
/// still exits on the first fault: a panic proves an invariant broke in the
/// read path, and an honestly dead lane (typed stop on later admission) beats
/// a worker silently retrying over the same broken code.
fn run_one_query(command: &QueryCommand, loader: &HistoryReplayLoader) -> bool {
    let loaded_at_ms = unix_now_ms();
    let (outcome, faulted) = match catch_worker_panic(|| loader(command.request, loaded_at_ms)) {
        Ok(Ok(rows)) => (HistoryReplayCompletionOutcome::Loaded(rows), false),
        Ok(Err(error)) => (HistoryReplayCompletionOutcome::Failed(error), false),
        Err(detail) => (
            HistoryReplayCompletionOutcome::Failed(runtime_error(
                HistoryReplayErrorKind::WorkerStopped,
                &detail,
            )),
            true,
        ),
    };
    // Every accepted request consumed one client-local credit and
    // that client's completion lane has the exact same capacity.
    // `Full` is therefore unreachable without violating the
    // admission invariant; `Disconnected` means the client went
    // away and this read-only result has no remaining authority.
    let _ = command.completion_tx.try_send(HistoryReplayCompletion {
        request: command.request,
        loaded_at_ms,
        outcome,
    });
    faulted
}

fn query_rows(
    query: &HistoryQuery,
    request: HistoryReplayRequest,
    now_ms: u64,
) -> Result<Arc<[HistoryReplayRow]>, HistoryReplayError> {
    let keys = query.known_series().map_err(map_store_error)?;
    let mut rows = Vec::with_capacity(keys.len());
    for key in keys {
        let Some(read) = query
            .series(&key, request.window(), now_ms)
            .map_err(map_store_error)?
        else {
            continue;
        };
        let peak = read.series.peak();
        let raw: Vec<f32> = read
            .series
            .samples
            .iter()
            .map(|sample| sample.value.map_or(f32::NAN, |value| value as f32))
            .collect();
        let positions = taskmanager_application::history_decimation::stride_envelope_positions(
            &raw,
            MAX_HISTORY_REPLAY_POINTS,
        );
        let samples = positions
            .iter()
            .filter_map(|position| raw.get(*position).copied())
            .collect::<Vec<_>>();
        let sample_times_ms = positions
            .iter()
            .filter_map(|position| {
                read.series
                    .samples
                    .get(*position)
                    .map(|sample| sample.completed_at_ms)
            })
            .collect::<Vec<_>>();
        rows.push(HistoryReplayRow {
            key,
            samples: Arc::from(samples),
            sample_times_ms: Arc::from(sample_times_ms),
            peak_value: peak.and_then(|peak| peak.value),
            peak_measured_at_ms: peak.and_then(|peak| peak.measured_at_ms),
            observed: read
                .series
                .samples
                .iter()
                .filter(|sample| !sample.is_gap())
                .count(),
            gaps: read.series.gap_count(),
            clock_jumps: read.series.clock_jumps,
        });
    }
    Ok(Arc::from(rows))
}

fn map_store_error(error: HistoryStoreError) -> HistoryReplayError {
    let kind = match error.kind() {
        HistoryStoreErrorKind::Decode => HistoryReplayErrorKind::Decode,
        HistoryStoreErrorKind::ResourceLimit => HistoryReplayErrorKind::ResourceLimit,
        HistoryStoreErrorKind::CreateDirectory
        | HistoryStoreErrorKind::Open
        | HistoryStoreErrorKind::Read
        | HistoryStoreErrorKind::Write
        | HistoryStoreErrorKind::Remove
        | HistoryStoreErrorKind::Rename
        | HistoryStoreErrorKind::Encode
        | HistoryStoreErrorKind::Locked => HistoryReplayErrorKind::Read,
    };
    runtime_error(kind, error.detail())
}

fn runtime_error(kind: HistoryReplayErrorKind, detail: &str) -> HistoryReplayError {
    HistoryReplayError::new(kind, Arc::<str>::from(detail))
}

fn unix_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "../tests/headless/history_replay_runtime_tests.rs"]
mod tests;
