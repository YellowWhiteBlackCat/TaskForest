//! Shared process-identity rejection contract.

use taskmanager_platform_contract::ProviderFailure;

/// Verify that a stale or mismatched process identity is rejected before the
/// native operator performs a side effect.
///
/// Adapter tests obtain `side_effects_before` and `side_effects_after` from an
/// injected operator/recorder. This helper intentionally does not infer safety
/// from a returned error alone: an adapter that mutates first and validates
/// later fails the contract.
pub fn assert_identity_change_is_side_effect_free<T>(
    result: &Result<T, ProviderFailure>,
    side_effects_before: u64,
    side_effects_after: u64,
) -> Result<(), String> {
    if !matches!(result, Err(ProviderFailure::IdentityChanged)) {
        return Err(format!(
            "wrong process identity returned {kind}, expected IdentityChanged",
            kind = provider_result_kind(result)
        ));
    }
    if side_effects_after != side_effects_before {
        return Err(format!(
            "wrong process identity changed native side-effect count from {side_effects_before} to {side_effects_after}"
        ));
    }
    Ok(())
}

fn provider_result_kind<T>(result: &Result<T, ProviderFailure>) -> String {
    match result {
        Ok(_) => "success".to_string(),
        Err(failure) => format!("{failure:?}"),
    }
}

#[cfg(test)]
#[path = "../tests/headless/identity_contract.rs"]
mod tests;
