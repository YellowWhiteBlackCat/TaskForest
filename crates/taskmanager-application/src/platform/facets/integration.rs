//! Desktop-integration capability ports and events.
//!
//! Defines command launch, resource reveal, URL open, and desktop appearance
//! requests plus the `IntegrationFacets` group of independently optional ports.

use std::path::PathBuf;
use std::sync::Arc;

use taskmanager_platform_contract::{
    CapabilityId, CapabilityRequest, CompositeSourceSnapshot, RequestPort, RequestScope,
    RequestTracking, RequestTrackingError,
};

use crate::{DesktopAppearance, FrozenProcessIdentity};
pub use taskmanager_core::{SetupScriptAction, SetupScriptEvent, SetupScriptInfo};

#[derive(Clone, Debug)]
pub enum ShellEvent {
    CommandLaunched {
        pid: u32,
    },
    TargetOpened,
    /// Echoed by the native adapter once a desktop notification was accepted
    /// by the platform notification service (BN-07). No user-visible event is
    /// produced; the echo proves delivery for the capability receipt.
    NotificationDelivered,
}

impl ShellEvent {
    #[must_use]
    pub fn accepts_capability(&self, capability: &CapabilityId) -> bool {
        match self {
            Self::CommandLaunched { .. } => capability == &CapabilityId::COMMAND_LAUNCH,
            Self::TargetOpened => {
                capability == &CapabilityId::RESOURCE_REVEAL
                    || capability == &CapabilityId::URL_OPEN
            }
            Self::NotificationDelivered => capability == &CapabilityId::DESKTOP_NOTIFY,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandLaunchRequest {
    pub command: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceRevealRequest {
    pub target: FrozenProcessIdentity,
    pub cached_executable: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UrlOpenRequest {
    pub url: String,
}

/// Desktop notification for a fired alert (extension capability
/// `alerts.notify`). `instance_id` is the alert
/// de-duplication key; upstream gating (policy/cooldown/quiet hours) is done
/// by the pure [`taskmanager_core::alerts::NotificationGate`] before this request is emitted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopNotificationRequest {
    pub instance_id: String,
    pub title: String,
    pub body: String,
    pub severity: crate::alerts::AlertSeverity,
    pub target: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetupScriptRequest {
    pub action: SetupScriptAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopAppearanceRequest {
    Observe,
}

bind_request_capability!(CommandLaunchRequest, CapabilityId::COMMAND_LAUNCH);
impl CapabilityRequest for ResourceRevealRequest {
    const CAPABILITY: CapabilityId = CapabilityId::RESOURCE_REVEAL;

    fn runtime_tracking(&self) -> Result<RequestTracking, RequestTrackingError> {
        let Some(start_token) = self.target.authoritative_start_token() else {
            return Err(RequestTrackingError::MissingTargetIdentity);
        };
        RequestScope::try_owned(format!("{}:{start_token}", self.target.pid))
            .map(RequestTracking::Target)
    }
}
bind_request_capability!(UrlOpenRequest, CapabilityId::URL_OPEN);
bind_request_capability!(DesktopNotificationRequest, CapabilityId::DESKTOP_NOTIFY);
bind_request_capability!(DesktopAppearanceRequest, CapabilityId::DESKTOP_APPEARANCE);
bind_request_capability!(SetupScriptRequest, CapabilityId::FIRST_RUN_SETUP);

#[derive(Clone, Debug)]
pub enum DesktopAppearanceEvent {
    Snapshot(CompositeSourceSnapshot<DesktopAppearance>),
}

impl DesktopAppearanceEvent {
    #[must_use]
    pub fn accepts_capability(&self, capability: &CapabilityId) -> bool {
        capability == &CapabilityId::DESKTOP_APPEARANCE
    }
}

pub type CommandLaunchRequestPort = dyn RequestPort<Request = CommandLaunchRequest>;
pub type ResourceRevealRequestPort = dyn RequestPort<Request = ResourceRevealRequest>;
pub type UrlOpenRequestPort = dyn RequestPort<Request = UrlOpenRequest>;
pub type DesktopNotificationRequestPort = dyn RequestPort<Request = DesktopNotificationRequest>;
pub type DesktopAppearanceRequestPort = dyn RequestPort<Request = DesktopAppearanceRequest>;
pub type SetupScriptRequestPort = dyn RequestPort<Request = SetupScriptRequest>;

/// Independently optional desktop-integration ports.
#[derive(Clone, Default)]
pub struct IntegrationFacets {
    command_launch: Option<Arc<CommandLaunchRequestPort>>,
    resource_reveal: Option<Arc<ResourceRevealRequestPort>>,
    url_open: Option<Arc<UrlOpenRequestPort>>,
    desktop_appearance: Option<Arc<DesktopAppearanceRequestPort>>,
    desktop_notification: Option<Arc<DesktopNotificationRequestPort>>,
    setup_script: Option<Arc<SetupScriptRequestPort>>,
}

impl IntegrationFacets {
    #[must_use]
    pub fn with_command_launch(mut self, port: Arc<CommandLaunchRequestPort>) -> Self {
        self.command_launch = Some(port);
        self
    }

    #[must_use]
    pub fn with_resource_reveal(mut self, port: Arc<ResourceRevealRequestPort>) -> Self {
        self.resource_reveal = Some(port);
        self
    }

    #[must_use]
    pub fn with_url_open(mut self, port: Arc<UrlOpenRequestPort>) -> Self {
        self.url_open = Some(port);
        self
    }

    #[must_use]
    pub fn with_desktop_appearance(mut self, port: Arc<DesktopAppearanceRequestPort>) -> Self {
        self.desktop_appearance = Some(port);
        self
    }

    #[must_use]
    pub fn with_desktop_notification(mut self, port: Arc<DesktopNotificationRequestPort>) -> Self {
        self.desktop_notification = Some(port);
        self
    }

    #[must_use]
    pub fn with_setup_script(mut self, port: Arc<SetupScriptRequestPort>) -> Self {
        self.setup_script = Some(port);
        self
    }

    #[must_use]
    pub fn command_launch(&self) -> Option<&CommandLaunchRequestPort> {
        self.command_launch.as_deref()
    }

    #[must_use]
    pub fn resource_reveal(&self) -> Option<&ResourceRevealRequestPort> {
        self.resource_reveal.as_deref()
    }

    #[must_use]
    pub fn url_open(&self) -> Option<&UrlOpenRequestPort> {
        self.url_open.as_deref()
    }

    #[must_use]
    pub fn desktop_appearance(&self) -> Option<&DesktopAppearanceRequestPort> {
        self.desktop_appearance.as_deref()
    }

    #[must_use]
    pub fn desktop_notification(&self) -> Option<&DesktopNotificationRequestPort> {
        self.desktop_notification.as_deref()
    }

    #[must_use]
    pub fn setup_script(&self) -> Option<&SetupScriptRequestPort> {
        self.setup_script.as_deref()
    }
}
