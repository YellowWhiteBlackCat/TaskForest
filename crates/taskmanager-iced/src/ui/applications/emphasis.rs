//! The process-control emphasis: the destructive accent the "End task"
//! action wears when the selected process is stuck in the platform's
//! "not responding" state.
//!
//! The typed signal is the core's uninterruptible wait (Linux `D`): a
//! process that cannot be interrupted and is usually blocked on I/O. It is
//! the state this product surfaces as "not responding" (the same state the
//! Applications subtitle counts as `proc.d_state`), and it is read from the
//! live process facts — never from a synthetic renderer flag.

use taskmanager_core::core::process::{ProcessItem, ProcessLiveKey};
use taskmanager_shell::ShellApp;

/// Whether one process is in the platform's "not responding" state.
///
/// The typed source is [`ProcessItem::status_kind`], the core's
/// provider-neutral process state: the uninterruptible wait (`D`) is kept
/// distinct from ordinary sleeping precisely because it denotes a process
/// that cannot be woken.
#[must_use]
pub(crate) fn is_not_responding(process: &ProcessItem) -> bool {
    process.status_kind().is_uninterruptible()
}

/// Whether the current selection contains a process in that state.
///
/// The selection is the shared shell's live identity set; the state is read
/// from the same process snapshot the table renders, so the emphasis can
/// never disagree with the row the user sees.
#[must_use]
pub(crate) fn selection_is_not_responding(shell: &ShellApp) -> bool {
    let selected = shell.selected_identities();
    if selected.is_empty() {
        return false;
    }
    shell
        .projection()
        .processes
        .as_deref()
        .is_some_and(|processes| {
            processes.iter().any(|process| {
                ProcessLiveKey::from_process(process).is_some_and(|key| selected.contains(&key))
                    && is_not_responding(process)
            })
        })
}
