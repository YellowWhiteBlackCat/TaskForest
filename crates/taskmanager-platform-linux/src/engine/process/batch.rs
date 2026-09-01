//! Bounded Linux worker for identity-safe process control intents.

#[cfg(feature = "test-support")]
use std::thread;

#[cfg(feature = "test-support")]
use crossbeam_channel::{Receiver, Sender, TrySendError, bounded};
use taskmanager_core::core::process::{
    ProcessBatchAction, ProcessBatchIntent, ProcessBatchResult, execute_process_batch_with,
};

use super::ProcessManager;

pub(crate) fn execute_process_batch(intent: ProcessBatchIntent) -> ProcessBatchResult {
    let mut manager = ProcessManager::new();
    let live = manager.refresh().items;
    execute_process_batch_with(intent, &live, |action, target| {
        super::validate_exact_start_token(target)?;
        let direct = match action {
            ProcessBatchAction::End | ProcessBatchAction::EndProcessTree => {
                ProcessManager::terminate_process(target.pid)
            }
            ProcessBatchAction::Kill => ProcessManager::kill_process(target.pid),
            ProcessBatchAction::Suspend => ProcessManager::pause_process(target.pid),
            ProcessBatchAction::Resume => ProcessManager::resume_process(target.pid),
            ProcessBatchAction::SetPriority(tier) => {
                ProcessManager::set_process_nice(target.pid, tier.canonical_nice())
            }
        };
        super::finish_with_escalation(target, super::batch_operation(action), direct)
    })
}

#[cfg(feature = "test-support")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessBatchSubmitError {
    Busy,
    Unavailable,
}

#[cfg(feature = "test-support")]
pub struct ProcessBatchWorker {
    request_tx: Sender<ProcessBatchIntent>,
    result_rx: Receiver<ProcessBatchResult>,
}

#[cfg(feature = "test-support")]
impl Default for ProcessBatchWorker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "test-support")]
impl ProcessBatchWorker {
    #[must_use]
    pub fn new() -> Self {
        let (request_tx, request_rx) = bounded::<ProcessBatchIntent>(1);
        let (result_tx, result_rx) = bounded::<ProcessBatchResult>(1);
        let _ = thread::Builder::new()
            .name("process-batch-worker".into())
            .spawn(move || {
                while let Ok(intent) = request_rx.recv() {
                    if result_tx.send(execute_process_batch(intent)).is_err() {
                        break;
                    }
                }
            });
        Self {
            request_tx,
            result_rx,
        }
    }

    pub fn submit(&self, intent: ProcessBatchIntent) -> Result<(), ProcessBatchSubmitError> {
        self.request_tx
            .try_send(intent)
            .map_err(|error| match error {
                TrySendError::Full(_) => ProcessBatchSubmitError::Busy,
                TrySendError::Disconnected(_) => ProcessBatchSubmitError::Unavailable,
            })
    }

    #[must_use]
    pub fn try_recv(&self) -> Option<ProcessBatchResult> {
        self.result_rx.try_recv().ok()
    }
}

#[cfg(all(test, feature = "test-support"))]
#[path = "../../../tests/headless/linux_engine_process_batch_tests.rs"]
mod tests;
