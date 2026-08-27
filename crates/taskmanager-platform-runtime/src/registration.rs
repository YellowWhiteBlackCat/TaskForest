//! Typed native-provider registration at the composition boundary.

use std::marker::PhantomData;

use taskmanager_application::{CapabilityRequest, CapabilityStatus, ProviderId};

/// Provider attribution tied to the application request type it serves.
///
/// The request marker is compile-time only. Runtime channels still carry the
/// stable provider identity, while capability ownership comes from
/// [`CapabilityRequest`].
pub struct ProviderBinding<R: CapabilityRequest> {
    provider: Option<ProviderId>,
    initial_status: CapabilityStatus,
    request: PhantomData<fn() -> R>,
}

impl<R: CapabilityRequest> ProviderBinding<R> {
    #[must_use]
    pub(crate) fn present(provider: ProviderId) -> Self {
        Self::present_with_status(provider, CapabilityStatus::TemporarilyUnavailable)
    }

    #[must_use]
    fn with_initial_status(mut self, initial_status: CapabilityStatus) -> Self {
        self.initial_status = initial_status;
        self
    }

    #[must_use]
    pub(crate) fn present_with_status(
        provider: ProviderId,
        initial_status: CapabilityStatus,
    ) -> Self {
        Self {
            provider: Some(provider),
            initial_status,
            request: PhantomData,
        }
    }

    #[must_use]
    pub const fn absent() -> Self {
        Self {
            provider: None,
            initial_status: CapabilityStatus::TemporarilyUnavailable,
            request: PhantomData,
        }
    }

    #[must_use]
    pub fn as_ref(&self) -> Option<&ProviderId> {
        self.provider.as_ref()
    }

    #[must_use]
    pub(crate) fn route_parts(&self) -> Option<(&ProviderId, CapabilityStatus)> {
        Some((self.provider.as_ref()?, self.initial_status))
    }
}

impl<R: CapabilityRequest> Clone for ProviderBinding<R> {
    fn clone(&self) -> Self {
        Self {
            provider: self.provider.clone(),
            initial_status: self.initial_status,
            request: PhantomData,
        }
    }
}

impl<R: CapabilityRequest> Default for ProviderBinding<R> {
    fn default() -> Self {
        Self::absent()
    }
}

/// One native provider object, its stable identity, and the request capability
/// it implements.
///
/// This value is intentionally generic over the provider representation. The
/// shared runtime neither imports provider SPI nor creates a trait bag; native
/// adapters may register a concrete implementation and erase it to their
/// platform-neutral provider trait only when building an executor.
pub struct ProviderRegistration<R: CapabilityRequest, P> {
    provider_id: ProviderId,
    provider: P,
    initial_status: CapabilityStatus,
    request: PhantomData<fn() -> R>,
}

impl<R: CapabilityRequest, P> ProviderRegistration<R, P> {
    #[must_use]
    pub const fn new(provider_id: ProviderId, provider: P) -> Self {
        Self {
            provider_id,
            provider,
            initial_status: CapabilityStatus::TemporarilyUnavailable,
            request: PhantomData,
        }
    }

    #[must_use]
    pub fn binding(&self) -> ProviderBinding<R> {
        ProviderBinding::present(self.provider_id.clone()).with_initial_status(self.initial_status)
    }

    /// Declare the exact status known at native composition time. The runtime
    /// catalog publishes this value before the first request, then normal
    /// provider health becomes authoritative after each correlated terminal.
    #[must_use]
    pub const fn with_initial_status(mut self, status: CapabilityStatus) -> Self {
        self.initial_status = status;
        self
    }

    #[must_use]
    pub fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    #[must_use]
    pub fn into_parts(self) -> (ProviderId, P) {
        (self.provider_id, self.provider)
    }

    #[must_use]
    pub fn into_provider(self) -> P {
        self.provider
    }

    #[must_use]
    pub fn map_provider<Q>(self, map: impl FnOnce(P) -> Q) -> ProviderRegistration<R, Q> {
        let initial_status = self.initial_status;
        let (provider_id, provider) = self.into_parts();
        ProviderRegistration::new(provider_id, map(provider)).with_initial_status(initial_status)
    }
}

/// A binding cannot cross request capability types even though both carry the
/// same runtime representation.
///
/// ```compile_fail
/// use taskmanager_application::{CommandLaunchRequest, ProviderId, UrlOpenRequest};
/// use taskmanager_platform_runtime::{ProviderBinding, ProviderRegistration};
///
/// let url = ProviderRegistration::<UrlOpenRequest, _>::new(
///     ProviderId::borrowed("fixture.url"),
///     (),
/// );
/// let _: ProviderBinding<CommandLaunchRequest> = url.binding();
/// ```
const _: () = ();

#[cfg(test)]
#[path = "../tests/headless/runtime_registration_tests.rs"]
mod tests;
