//! Shared Linux policy for revalidating a frozen process target before use.

use taskforest_contract::ProviderFailure;
use taskforest_core::core::failure::FailureKind;
use taskforest_core::core::process::FrozenProcessIdentity;
use taskforest_core::core::source::{SourceOutcome, SourceStatus};

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
        .reduce(FailureKind::stronger)
        .map(ProviderFailure::from_kind)
        .unwrap_or(ProviderFailure::ProviderFault)
}

#[cfg(test)]
#[path = "../../tests/headless/linux_provider_process_target_tests.rs"]
mod tests;
