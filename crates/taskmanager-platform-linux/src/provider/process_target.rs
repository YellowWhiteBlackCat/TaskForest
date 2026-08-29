//! Shared Linux policy for revalidating a frozen process target before use.

use taskmanager_core::core::process::FrozenProcessIdentity;
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_platform_contract::ProviderFailure;

use crate::engine::process::ProcessManager;

pub(super) fn validate_process_identity(
    manager: &mut ProcessManager,
    target: &FrozenProcessIdentity,
) -> Result<(), ProviderFailure> {
    let expected_start_token = target
        .authoritative_start_token()
        .ok_or(ProviderFailure::IdentityChanged)?;
    let snapshot = manager.refresh();
    if let Some(failure) = inventory_failure(&snapshot.sources) {
        return Err(failure);
    }
    let live = snapshot.items;
    let Some(process) = live.iter().find(|process| process.pid == target.pid) else {
        return Err(ProviderFailure::IdentityChanged);
    };
    let Some(current_start_token) = process.current_start_token() else {
        return Err(snapshot_failure(&snapshot.sources));
    };
    if process.name != target.name || current_start_token != expected_start_token {
        return Err(ProviderFailure::IdentityChanged);
    }
    Ok(())
}

fn inventory_failure(sources: &[SourceStatus]) -> Option<ProviderFailure> {
    const INVENTORY_PROVIDER: &str = "linux.process.procfs.inventory";
    match sources
        .iter()
        .find(|source| source.provider.as_str() == INVENTORY_PROVIDER)
        .map(|source| source.outcome)
    {
        Some(SourceOutcome::Available | SourceOutcome::Empty) => None,
        Some(SourceOutcome::Partial(failure) | SourceOutcome::Unavailable(failure)) => {
            Some(ProviderFailure::from_kind(failure))
        }
        None => Some(ProviderFailure::ProviderFault),
    }
}

fn snapshot_failure(sources: &[SourceStatus]) -> ProviderFailure {
    sources
        .iter()
        .filter_map(|source| match source.outcome {
            SourceOutcome::Partial(failure) | SourceOutcome::Unavailable(failure) => Some(failure),
            SourceOutcome::Available | SourceOutcome::Empty => None,
        })
        .max_by_key(|failure| failure_priority(*failure))
        .map(ProviderFailure::from_kind)
        .unwrap_or(ProviderFailure::ProviderFault)
}

const fn failure_priority(failure: taskmanager_core::FailureKind) -> u8 {
    use taskmanager_core::FailureKind;
    match failure {
        FailureKind::RequiresEscalation => 9,
        FailureKind::PermissionDenied => 8,
        FailureKind::MissingDependency => 7,
        FailureKind::TimedOut => 6,
        FailureKind::ProviderFault => 5,
        FailureKind::TemporarilyUnavailable => 4,
        FailureKind::Unsupported => 3,
        FailureKind::IdentityChanged => 2,
        FailureKind::Rejected => 1,
    }
}

#[cfg(test)]
#[path = "../../tests/headless/linux_provider_process_target_tests.rs"]
mod tests;
