//! One multi-level tree for normal frozen-closure capture and behavior checks.

use crate::fixture::{ProjectionSeedFact, seed_projection_fact};
use crate::{ProcessRowId, ShellApp};
use taskmanager_core::core::ScalarObservation;
use taskmanager_core::core::process::{ProcessItem, ProcessLiveKey, ProcessScalarObservations};

pub fn append_process_tree(processes: &mut Vec<ProcessItem>) {
    processes.retain(|process| !(90_000..=90_006).contains(&process.pid));
    for offset in 0..=6 {
        let mut process = ProcessItem::new(
            90_000 + offset,
            if offset == 0 {
                "capture-app".to_owned()
            } else {
                format!("capture-worker-{offset}")
            },
        );
        process.parent_pid = match offset {
            0 => None,
            1 | 2 => Some(90_000),
            _ => Some(90_000 + offset - 2),
        };
        process.apply_scalar_observations(ProcessScalarObservations {
            start_token: ScalarObservation::available(9_000_000 + u64::from(offset), 1),
            ..Default::default()
        });
        processes.push(process);
    }
}

pub fn seed_shell_process_tree(shell: &mut ShellApp) -> Option<ProcessLiveKey> {
    let mut processes = shell
        .projection()
        .processes
        .as_deref()
        .cloned()
        .unwrap_or_default();
    append_process_tree(&mut processes);
    let root = processes
        .iter()
        .find(|process| process.pid == 90_000)?
        .clone();
    let identity = ProcessLiveKey::from_process(&root)?;
    seed_projection_fact(shell, ProjectionSeedFact::Processes(Some(processes)));
    shell.set_row_selection(ProcessRowId::from_process(&root), Some(&root));
    Some(identity)
}
