//! OS-neutral desktop-shell integration contracts and typed lane routing.

use std::path::PathBuf;
use std::sync::Arc;

use crossbeam_channel::Receiver;
use taskmanager_application::{
    CommandLaunchRequest, DesktopAppearanceEvent, DesktopAppearanceRequest,
    DesktopNotificationRequest, PlatformEvent, ResourceRevealRequest, SetupScriptRequest,
    ShellEvent, UrlOpenRequest,
};
use taskmanager_core::core::appearance::DesktopAppearance;
use taskmanager_core::core::process::FrozenProcessIdentity;
use taskmanager_core::core::setup::{SetupScriptAction, SetupScriptEvent};
use taskmanager_platform_contract::{CapabilityId, CompositeSourceSnapshot, ProviderFailure};

use crate::{
    Queued, RuntimeEventPublisher, WorkerRuntime, WorkerSpawnError, spawn_lane,
    spawn_observation_lane,
};

type CommandLaunchExecutor = dyn FnMut(String) -> Result<u32, ProviderFailure> + Send + 'static;
type ResourceRevealExecutor = dyn FnMut(FrozenProcessIdentity, Option<PathBuf>) -> Result<(), ProviderFailure>
    + Send
    + 'static;
type UrlOpenExecutor = dyn FnMut(String) -> Result<(), ProviderFailure> + Send + 'static;
type DesktopAppearanceExecutor = dyn FnMut() -> Result<CompositeSourceSnapshot<DesktopAppearance>, ProviderFailure>
    + Send
    + 'static;
type SetupScriptExecutor =
    dyn FnMut(SetupScriptAction) -> Result<SetupScriptEvent, ProviderFailure> + Send + 'static;
type DesktopNotificationExecutor =
    dyn FnMut(DesktopNotificationRequest) -> Result<(), ProviderFailure> + Send + 'static;

/// Native host-integration operations adapted into OS-independent closures.
pub struct IntegrationExecutors {
    command_launch: Box<CommandLaunchExecutor>,
    resource_reveal: Box<ResourceRevealExecutor>,
    url_open: Box<UrlOpenExecutor>,
    desktop_appearance: Box<DesktopAppearanceExecutor>,
    desktop_notification: Option<Box<DesktopNotificationExecutor>>,
    setup_script: Option<Box<SetupScriptExecutor>>,
}

impl IntegrationExecutors {
    #[must_use]
    pub fn new<C, R, U, D>(
        command_launch: C,
        resource_reveal: R,
        url_open: U,
        desktop_appearance: D,
    ) -> Self
    where
        C: FnMut(String) -> Result<u32, ProviderFailure> + Send + 'static,
        R: FnMut(FrozenProcessIdentity, Option<PathBuf>) -> Result<(), ProviderFailure>
            + Send
            + 'static,
        U: FnMut(String) -> Result<(), ProviderFailure> + Send + 'static,
        D: FnMut() -> Result<CompositeSourceSnapshot<DesktopAppearance>, ProviderFailure>
            + Send
            + 'static,
    {
        Self {
            command_launch: Box::new(command_launch),
            resource_reveal: Box::new(resource_reveal),
            url_open: Box::new(url_open),
            desktop_appearance: Box::new(desktop_appearance),
            desktop_notification: None,
            setup_script: None,
        }
    }

    #[must_use]
    pub fn with_desktop_notification<N>(mut self, desktop_notification: N) -> Self
    where
        N: FnMut(DesktopNotificationRequest) -> Result<(), ProviderFailure> + Send + 'static,
    {
        self.desktop_notification = Some(Box::new(desktop_notification));
        self
    }

    #[must_use]
    pub fn with_setup_script<S>(mut self, setup_script: S) -> Self
    where
        S: FnMut(SetupScriptAction) -> Result<SetupScriptEvent, ProviderFailure> + Send + 'static,
    {
        self.setup_script = Some(Box::new(setup_script));
        self
    }
}

/// Optional integration receivers while native capability bindings are assembled.
pub struct PendingIntegrationRuntimeLanes {
    pub command_launch_rx: Option<Receiver<Queued<CommandLaunchRequest>>>,
    pub resource_reveal_rx: Option<Receiver<Queued<ResourceRevealRequest>>>,
    pub url_open_rx: Option<Receiver<Queued<UrlOpenRequest>>>,
    pub desktop_appearance_rx: Option<Receiver<Queued<DesktopAppearanceRequest>>>,
    pub desktop_notification_rx: Option<Receiver<Queued<DesktopNotificationRequest>>>,
    pub setup_script_rx: Option<Receiver<Queued<SetupScriptRequest>>>,
}

impl PendingIntegrationRuntimeLanes {
    pub(crate) fn new(
        command_launch_rx: Option<Receiver<Queued<CommandLaunchRequest>>>,
        resource_reveal_rx: Option<Receiver<Queued<ResourceRevealRequest>>>,
        url_open_rx: Option<Receiver<Queued<UrlOpenRequest>>>,
        desktop_appearance_rx: Option<Receiver<Queued<DesktopAppearanceRequest>>>,
        desktop_notification_rx: Option<Receiver<Queued<DesktopNotificationRequest>>>,
        setup_script_rx: Option<Receiver<Queued<SetupScriptRequest>>>,
    ) -> Self {
        Self {
            command_launch_rx,
            resource_reveal_rx,
            url_open_rx,
            desktop_appearance_rx,
            desktop_notification_rx,
            setup_script_rx,
        }
    }

    /// Required-lane gaps only. `DESKTOP_NOTIFY` (and `FIRST_RUN_SETUP`) are
    /// optional facets — their lanes are `Option` and `try_complete` promotes
    /// the family without them — so their absence is not a composition gap.
    pub(crate) fn missing_capabilities(&self) -> impl Iterator<Item = CapabilityId> {
        [
            (
                self.command_launch_rx.is_none(),
                CapabilityId::COMMAND_LAUNCH,
            ),
            (
                self.resource_reveal_rx.is_none(),
                CapabilityId::RESOURCE_REVEAL,
            ),
            (self.url_open_rx.is_none(), CapabilityId::URL_OPEN),
            (
                self.desktop_appearance_rx.is_none(),
                CapabilityId::DESKTOP_APPEARANCE,
            ),
        ]
        .into_iter()
        .filter_map(|(is_missing, capability)| is_missing.then_some(capability))
    }

    /// Promote the integration family only when all four lanes exist.
    #[must_use]
    pub fn try_complete(self) -> Option<IntegrationRuntimeLanes> {
        let Self {
            command_launch_rx: Some(command_launch),
            resource_reveal_rx: Some(resource_reveal),
            url_open_rx: Some(url_open),
            desktop_appearance_rx: Some(desktop_appearance),
            desktop_notification_rx,
            setup_script_rx,
        } = self
        else {
            return None;
        };
        Some(IntegrationRuntimeLanes {
            command_launch,
            resource_reveal,
            url_open,
            desktop_appearance,
            desktop_notification: desktop_notification_rx,
            setup_script: setup_script_rx,
        })
    }
}

/// Complete provider-side receivers for the host-integration capability family.
pub struct IntegrationRuntimeLanes {
    command_launch: Receiver<Queued<CommandLaunchRequest>>,
    resource_reveal: Receiver<Queued<ResourceRevealRequest>>,
    url_open: Receiver<Queued<UrlOpenRequest>>,
    desktop_appearance: Receiver<Queued<DesktopAppearanceRequest>>,
    desktop_notification: Option<Receiver<Queued<DesktopNotificationRequest>>>,
    setup_script: Option<Receiver<Queued<SetupScriptRequest>>>,
}

/// Attach all host-integration executors to their independent typed lanes.
pub fn spawn_integration_lanes(
    workers: &WorkerRuntime,
    lanes: IntegrationRuntimeLanes,
    executors: IntegrationExecutors,
    events: Arc<RuntimeEventPublisher>,
) -> Result<(), WorkerSpawnError> {
    let IntegrationRuntimeLanes {
        command_launch,
        resource_reveal,
        url_open,
        desktop_appearance,
        desktop_notification,
        setup_script,
    } = lanes;
    let IntegrationExecutors {
        command_launch: mut execute_command_launch,
        resource_reveal: mut execute_resource_reveal,
        url_open: mut execute_url_open,
        desktop_appearance: mut execute_desktop_appearance,
        desktop_notification: execute_desktop_notification,
        setup_script: execute_setup_script,
    } = executors;

    spawn_lane(
        workers,
        command_launch,
        events.clone(),
        move |CommandLaunchRequest { command }| {
            Ok(PlatformEvent::Shell(ShellEvent::CommandLaunched {
                pid: execute_command_launch(command)?,
            }))
        },
    )?;
    spawn_lane(
        workers,
        resource_reveal,
        events.clone(),
        move |ResourceRevealRequest {
                  target,
                  cached_executable,
              }| {
            execute_resource_reveal(target, cached_executable)?;
            Ok(PlatformEvent::Shell(ShellEvent::TargetOpened))
        },
    )?;
    spawn_lane(
        workers,
        url_open,
        events.clone(),
        move |UrlOpenRequest { url }| {
            execute_url_open(url)?;
            Ok(PlatformEvent::Shell(ShellEvent::TargetOpened))
        },
    )?;
    spawn_observation_lane(
        workers,
        desktop_appearance,
        events.clone(),
        move |DesktopAppearanceRequest::Observe| execute_desktop_appearance(),
        |snapshot| PlatformEvent::DesktopAppearance(DesktopAppearanceEvent::Snapshot(snapshot)),
    )?;
    if let (Some(receiver), Some(mut execute)) = (setup_script, execute_setup_script) {
        spawn_lane(
            workers,
            receiver,
            events.clone(),
            move |SetupScriptRequest { action }| execute(action).map(PlatformEvent::SetupScript),
        )?;
    }
    if let (Some(receiver), Some(mut execute)) =
        (desktop_notification, execute_desktop_notification)
    {
        spawn_lane(workers, receiver, events.clone(), move |request| {
            execute(request)?;
            Ok(PlatformEvent::Shell(ShellEvent::NotificationDelivered))
        })?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/headless/integration.rs"]
mod tests;
