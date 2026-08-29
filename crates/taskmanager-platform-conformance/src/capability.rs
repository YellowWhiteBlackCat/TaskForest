//! Capability-catalog honesty scenarios shared by adapters.

use std::collections::BTreeSet;

use taskmanager_core::core::identity::ProviderId;
use taskmanager_platform_contract::{CapabilityId, CapabilitySnapshot, CapabilityStatus};

/// A freshly spawned runtime must expose exactly the declared surface, with
/// descriptors that never claim availability before the first observation and
/// always attribute each capability to its owning provider identity.
pub fn assert_fresh_surface_descriptors(
    snapshot: &CapabilitySnapshot,
    surface: &[(&'static str, &'static str)],
    provider_prefix: &str,
) -> Result<(), String> {
    let declared: BTreeSet<&str> = surface.iter().map(|(id, _)| *id).collect();
    let observed: BTreeSet<&str> = snapshot
        .iter()
        .map(|descriptor| descriptor.id.as_str())
        .collect();
    if observed != declared {
        return Err(format!(
            "capability surface drifted: declared={declared:?} observed={observed:?}"
        ));
    }

    for (capability, provider) in surface {
        let Some(descriptor) = snapshot.get(&CapabilityId::borrowed(capability)) else {
            return Err(format!("missing capability descriptor {capability}"));
        };
        if descriptor.status != CapabilityStatus::TemporarilyUnavailable {
            return Err(format!(
                "fresh {capability} descriptor must not claim availability"
            ));
        }
        if !provider.starts_with(provider_prefix) {
            return Err(format!(
                "{capability} attributed to {provider}, expected {provider_prefix} prefix"
            ));
        }
        if descriptor.providers != [ProviderId::borrowed(provider)] {
            return Err(format!(
                "{capability} must be owned by its {provider_prefix} provider"
            ));
        }
        if descriptor.last_success_at_ms.is_some() {
            return Err(format!(
                "fresh {capability} descriptor must not carry a last-success timestamp"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/headless/capability_contract.rs"]
mod tests;
