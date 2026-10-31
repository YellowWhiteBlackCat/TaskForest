//! One multi-level tree for normal frozen-closure capture and behavior checks.

use crate::fixture::{ProjectionSeedFact, seed_projection_fact};
use crate::{ProcessRowId, ShellApp};
use taskforest_core::core::ScalarObservation;
use taskforest_core::core::failure::FailureKind;
use taskforest_core::core::process::{
    ProcessApplicationIdentity, ProcessItem, ProcessLiveKey, ProcessMetadataObservation,
    ProcessScalarObservations,
};

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

/// Seed the Applications aggregate capture with one typed permission failure.
///
/// The seven process rows share one verified application identity so all four
/// frontends render the same category and application totals. Six members
/// carry current CPU/memory values; one reports a permission failure, making
/// both aggregate cells visibly partial while retaining current coverage.
pub fn seed_shell_process_tree_with_partial_aggregate(
    shell: &mut ShellApp,
) -> Option<ProcessLiveKey> {
    let application =
        ProcessApplicationIdentity::new("org.taskforest.capture", "Capture Browser", None)?;
    let root = seed_shell_process_tree(shell)?;
    let mut processes = shell.projection().processes.as_deref()?.to_vec();
    let mut seeded = 0;
    for process in processes
        .iter_mut()
        .filter(|process| (90_000..=90_006).contains(&process.pid))
    {
        let offset = usize::try_from(process.pid - 90_000).ok()?;
        let cpu = [18.0, 12.5, 7.5, 4.0, 2.0, 1.0, 0.5][offset];
        let memory = [512, 128, 64, 32, 16, 8, 4][offset] * 1024 * 1024;
        let unavailable = offset == 2;
        let mut scalars = *process.scalar_observations();
        scalars.cpu_percentage = if unavailable {
            ScalarObservation::unavailable(FailureKind::PermissionDenied)
        } else {
            ScalarObservation::available(cpu, 1)
        };
        scalars.memory_bytes = if unavailable {
            ScalarObservation::unavailable(FailureKind::PermissionDenied)
        } else {
            ScalarObservation::available(memory, 1)
        };
        process.apply_scalar_observations(scalars);
        process.apply_application_identity(ProcessMetadataObservation::available(
            application.clone(),
            1,
        ));
        seeded += 1;
    }
    if seeded != 7 {
        return None;
    }
    seed_projection_fact(shell, ProjectionSeedFact::Processes(Some(processes)));
    Some(root)
}
