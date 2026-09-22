//! Capability-catalog honesty scenarios shared by adapters.

use std::collections::BTreeSet;

use taskmanager_core::core::identity::ProviderId;
use taskmanager_platform_contract::{
    CapabilityId, CapabilitySnapshot, CapabilityStatus, PlatformAxis, PlatformCapabilitySurface,
    PlatformSource,
};

/// A freshly spawned runtime must expose the complete product-expected
/// capability surface (M3.2 semantics): the registration face it DECLARES,
/// plus a typed absence for every product-expected identity it does not
/// register. A registered descriptor never claims availability before the
/// first observation and always attributes its capability to its owning
/// provider identity; an absent identity never disappears.
///
/// The registered face must equal the declared face exactly, in both
/// directions: a silently registered lane is as much a contract violation as a
/// declared lane that silently vanished. Typed absences are only admissible for
/// product-expected identities (see [`CapabilityId::EXPECTED_SURFACE`]).
pub fn assert_fresh_surface_descriptors(
    snapshot: &CapabilitySnapshot,
    surface: &[(&'static str, &'static str)],
    provider_prefix: &str,
) -> Result<(), String> {
    let declared: BTreeSet<&str> = surface.iter().map(|(id, _)| *id).collect();
    let registered: BTreeSet<&str> = snapshot
        .registered()
        .map(|descriptor| descriptor.id.as_str())
        .collect();
    if registered != declared {
        let undeclared: Vec<&str> = registered.difference(&declared).copied().collect();
        let hidden: Vec<&str> = declared.difference(&registered).copied().collect();
        return Err(format!(
            "registration face drifted: undeclared={undeclared:?} hidden={hidden:?}"
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

    // Every product-expected identity the adapter did not register answers with
    // its typed absence: no silent omission, no fabricated provider, no
    // fabricated observation or success.
    for expected in CapabilityId::EXPECTED_SURFACE {
        if declared.contains(expected.as_str()) {
            continue;
        }
        let Some(descriptor) = snapshot.get(&expected) else {
            return Err(format!(
                "unregistered expected capability {expected} must stay addressable"
            ));
        };
        if !descriptor.is_typed_absence() {
            return Err(format!(
                "{expected} must answer with its typed absence, got {:?}",
                descriptor.status
            ));
        }
        if descriptor.observed_at_ms != 0 {
            return Err(format!(
                "typed absence {expected} must not carry an observation time"
            ));
        }
        if descriptor.last_success_at_ms.is_some() {
            return Err(format!(
                "typed absence {expected} must not carry a last-success timestamp"
            ));
        }
    }

    // A typed absence is a product-surface answer; a vendor/diagnostic identity
    // is never silently absent because the product never promised it.
    for descriptor in snapshot.typed_absences() {
        if !descriptor.id.is_expected() {
            return Err(format!(
                "typed absence {} is outside the product-expected surface",
                descriptor.id
            ));
        }
    }
    Ok(())
}

/// Cross-check one platform's layer-B declaration ([`PlatformCapabilitySurface`])
/// against the live capability catalog (layer C).
///
/// This is the M3.2 "same source" scenario for the three-axis ledger: the
/// static declaration an adapter co-locates with its real route/provider
/// registration must agree with what its runtime catalog reports, in both
/// directions.
///
/// Checked:
///
/// 1. every product-expected identity carries a declaration on `platform` -
///    [`PlatformSource::Undeclared`] is the forbidden silent absence;
/// 2. a [`PlatformSource::Present`] declaration is backed by a registered
///    descriptor whose provider attribution carries the `provider_prefix`;
/// 3. every registered lane names its provider, and no product-expected
///    registration is silently undeclared;
/// 4. typed absences stay inside the product-expected surface.
///
/// Not checked, by design: whether a registered lane is a real source or a
/// registered-pending one. The runtime catalog publishes both as a registered
/// descriptor with a provider (the pending lane's typed `Unsupported` only
/// appears when it is requested), so the split lives in the adapter's
/// declaration, where it is co-located with the registration, and its
/// behavioral proof stays with the adapter's own typed-outcome contract test.
///
/// # Errors
///
/// Returns the first disagreement as a human-readable message.
pub fn assert_capability_surface_matches_catalog(
    snapshot: &CapabilitySnapshot,
    surface: &PlatformCapabilitySurface,
    platform: PlatformAxis,
    provider_prefix: &str,
) -> Result<(), String> {
    for expected in CapabilityId::EXPECTED_SURFACE {
        let declared = surface.source(platform, &expected);
        if declared == PlatformSource::Undeclared {
            return Err(format!(
                "expected capability {expected} is undeclared on {platform}"
            ));
        }
        let Some(descriptor) = snapshot.get(&expected) else {
            return Err(format!(
                "expected capability {expected} must stay addressable on {platform}"
            ));
        };
        if declared == PlatformSource::Present {
            if descriptor.is_typed_absence() {
                return Err(format!(
                    "{expected} is declared Present on {platform} but the catalog answers with its typed absence"
                ));
            }
            if descriptor.providers.is_empty() {
                return Err(format!(
                    "{expected} is declared Present on {platform} without a provider attribution"
                ));
            }
            if !descriptor
                .providers
                .iter()
                .any(|provider| provider.as_str().starts_with(provider_prefix))
            {
                return Err(format!(
                    "{expected} is registered on {platform} without a {provider_prefix} provider identity"
                ));
            }
        }
    }

    for descriptor in snapshot.registered() {
        if descriptor.providers.is_empty() {
            return Err(format!(
                "registered {} has no provider attribution on {platform}",
                descriptor.id
            ));
        }
        if !descriptor.id.is_expected() {
            continue; // vendor/diagnostic identities are outside the product promise
        }
        if surface.source(platform, &descriptor.id) == PlatformSource::Undeclared {
            return Err(format!(
                "{} is registered on {platform} but silently undeclared",
                descriptor.id
            ));
        }
    }

    for descriptor in snapshot.typed_absences() {
        if !descriptor.id.is_expected() {
            return Err(format!(
                "typed absence {} is outside the product-expected surface",
                descriptor.id
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/headless/capability_contract.rs"]
mod tests;
