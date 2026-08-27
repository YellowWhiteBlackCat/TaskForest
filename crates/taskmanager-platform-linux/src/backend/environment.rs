//! Linux startup and session providers bound to shared environment executors.
//!
//! Owns `EnvironmentProviders`, which adapts the five startup/session
//! registrations into `EnvironmentExecutors` and `EnvironmentProviderBindings`.

use taskmanager_application::{
    SessionControlRequest, SessionInventoryRequest, StartupControlRequest, StartupEvidenceRequest,
    StartupInventoryRequest,
};
use taskmanager_platform_provider::{
    SessionControlProvider, SessionInventoryProvider, StartupControlProvider,
    StartupEvidenceProvider, StartupInventoryProvider,
};
use taskmanager_platform_runtime::{
    EnvironmentExecutors, EnvironmentProviderBindings, ProviderRegistration,
};

type StartupInventoryRegistration =
    ProviderRegistration<StartupInventoryRequest, Box<dyn StartupInventoryProvider>>;
type StartupEvidenceRegistration =
    ProviderRegistration<StartupEvidenceRequest, Box<dyn StartupEvidenceProvider>>;
type StartupControlRegistration =
    ProviderRegistration<StartupControlRequest, Box<dyn StartupControlProvider>>;
type SessionInventoryRegistration =
    ProviderRegistration<SessionInventoryRequest, Box<dyn SessionInventoryProvider>>;
type SessionControlRegistration =
    ProviderRegistration<SessionControlRequest, Box<dyn SessionControlProvider>>;

/// Linux providers adapted to shared startup and session executors.
pub struct EnvironmentProviders {
    startup_inventory: StartupInventoryRegistration,
    startup_evidence: StartupEvidenceRegistration,
    startup_control: StartupControlRegistration,
    session_inventory: SessionInventoryRegistration,
    session_control: SessionControlRegistration,
}

impl EnvironmentProviders {
    #[must_use]
    pub fn new<I, E, C, S, M>(
        startup_inventory: ProviderRegistration<StartupInventoryRequest, I>,
        startup_evidence: ProviderRegistration<StartupEvidenceRequest, E>,
        startup_control: ProviderRegistration<StartupControlRequest, C>,
        session_inventory: ProviderRegistration<SessionInventoryRequest, S>,
        session_control: ProviderRegistration<SessionControlRequest, M>,
    ) -> Self
    where
        I: StartupInventoryProvider,
        E: StartupEvidenceProvider,
        C: StartupControlProvider,
        S: SessionInventoryProvider,
        M: SessionControlProvider,
    {
        Self {
            startup_inventory: startup_inventory
                .map_provider(|provider| Box::new(provider) as Box<dyn StartupInventoryProvider>),
            startup_evidence: startup_evidence
                .map_provider(|provider| Box::new(provider) as Box<dyn StartupEvidenceProvider>),
            startup_control: startup_control
                .map_provider(|provider| Box::new(provider) as Box<dyn StartupControlProvider>),
            session_inventory: session_inventory
                .map_provider(|provider| Box::new(provider) as Box<dyn SessionInventoryProvider>),
            session_control: session_control
                .map_provider(|provider| Box::new(provider) as Box<dyn SessionControlProvider>),
        }
    }

    pub(crate) fn runtime_bindings(&self) -> EnvironmentProviderBindings {
        EnvironmentProviderBindings::from_registrations(
            &self.startup_inventory,
            &self.startup_evidence,
            &self.startup_control,
            &self.session_inventory,
            &self.session_control,
        )
    }

    pub(crate) fn into_runtime(self) -> EnvironmentExecutors {
        let Self {
            startup_inventory,
            startup_evidence,
            startup_control,
            session_inventory,
            session_control,
        } = self;
        let mut startup_inventory = startup_inventory.into_provider();
        let mut startup_evidence = startup_evidence.into_provider();
        let mut startup_control = startup_control.into_provider();
        let mut session_inventory = session_inventory.into_provider();
        let mut session_control = session_control.into_provider();
        EnvironmentExecutors::new(
            move || startup_inventory.refresh(),
            move |observed_at_ms| startup_evidence.observe(observed_at_ms),
            move |entry, enabled| startup_control.set_enabled(entry, enabled),
            move || session_inventory.refresh(),
            move |session_id, action| session_control.control(session_id, action),
        )
    }
}
