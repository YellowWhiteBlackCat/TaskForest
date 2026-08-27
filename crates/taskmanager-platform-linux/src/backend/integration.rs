//! Linux host-integration providers bound to shared `IntegrationExecutors`.
//!
//! Owns `IntegrationProviders`, which adapts command launch, resource reveal,
//! URL open, and desktop appearance into `IntegrationProviderBindings`.

use taskmanager_application::{
    CommandLaunchRequest, DesktopAppearanceRequest, DesktopNotificationRequest,
    ResourceRevealRequest, SetupScriptRequest, UrlOpenRequest,
};
use taskmanager_platform_provider::{
    CommandLaunchProvider, DesktopAppearanceProvider, DesktopNotificationProvider,
    ResourceRevealProvider, SetupScriptProvider, UrlOpenProvider,
};
use taskmanager_platform_runtime::{
    IntegrationExecutors, IntegrationProviderBindings, ProviderRegistration,
};

type CommandLaunchRegistration =
    ProviderRegistration<CommandLaunchRequest, Box<dyn CommandLaunchProvider>>;
type ResourceRevealRegistration =
    ProviderRegistration<ResourceRevealRequest, Box<dyn ResourceRevealProvider>>;
type UrlOpenRegistration = ProviderRegistration<UrlOpenRequest, Box<dyn UrlOpenProvider>>;
type DesktopAppearanceRegistration =
    ProviderRegistration<DesktopAppearanceRequest, Box<dyn DesktopAppearanceProvider>>;
type SetupScriptRegistration =
    ProviderRegistration<SetupScriptRequest, Box<dyn SetupScriptProvider>>;
type DesktopNotificationRegistration =
    ProviderRegistration<DesktopNotificationRequest, Box<dyn DesktopNotificationProvider>>;

/// Linux provider implementations adapted to shared host-integration executors.
pub struct IntegrationProviders {
    command_launch: CommandLaunchRegistration,
    resource_reveal: ResourceRevealRegistration,
    url_open: UrlOpenRegistration,
    desktop_appearance: DesktopAppearanceRegistration,
    desktop_notification: Option<DesktopNotificationRegistration>,
    setup_script: Option<SetupScriptRegistration>,
}

impl IntegrationProviders {
    #[must_use]
    pub fn new<C, R, U, D>(
        command_launch: ProviderRegistration<CommandLaunchRequest, C>,
        resource_reveal: ProviderRegistration<ResourceRevealRequest, R>,
        url_open: ProviderRegistration<UrlOpenRequest, U>,
        desktop_appearance: ProviderRegistration<DesktopAppearanceRequest, D>,
    ) -> Self
    where
        C: CommandLaunchProvider,
        R: ResourceRevealProvider,
        U: UrlOpenProvider,
        D: DesktopAppearanceProvider,
    {
        Self {
            command_launch: command_launch
                .map_provider(|provider| Box::new(provider) as Box<dyn CommandLaunchProvider>),
            resource_reveal: resource_reveal
                .map_provider(|provider| Box::new(provider) as Box<dyn ResourceRevealProvider>),
            url_open: url_open
                .map_provider(|provider| Box::new(provider) as Box<dyn UrlOpenProvider>),
            desktop_appearance: desktop_appearance
                .map_provider(|provider| Box::new(provider) as Box<dyn DesktopAppearanceProvider>),
            desktop_notification: None,
            setup_script: None,
        }
    }

    #[must_use]
    pub fn with_desktop_notification(
        mut self,
        desktop_notification: ProviderRegistration<
            DesktopNotificationRequest,
            impl DesktopNotificationProvider,
        >,
    ) -> Self {
        self.desktop_notification =
            Some(desktop_notification.map_provider(|provider| {
                Box::new(provider) as Box<dyn DesktopNotificationProvider>
            }));
        self
    }

    #[must_use]
    pub fn with_setup_script<P>(
        mut self,
        setup_script: ProviderRegistration<SetupScriptRequest, P>,
    ) -> Self
    where
        P: SetupScriptProvider,
    {
        self.setup_script = Some(
            setup_script
                .map_provider(|provider| Box::new(provider) as Box<dyn SetupScriptProvider>),
        );
        self
    }

    pub(crate) fn runtime_bindings(&self) -> IntegrationProviderBindings {
        let bindings = IntegrationProviderBindings::from_registrations(
            &self.command_launch,
            &self.resource_reveal,
            &self.url_open,
            &self.desktop_appearance,
        );
        let bindings = self
            .desktop_notification
            .as_ref()
            .map_or(bindings.clone(), |notification| {
                bindings.with_desktop_notification(notification)
            });
        self.setup_script
            .as_ref()
            .map_or(bindings.clone(), |setup| bindings.with_setup_script(setup))
    }

    pub(crate) fn into_runtime(self) -> IntegrationExecutors {
        let Self {
            command_launch,
            resource_reveal,
            url_open,
            desktop_appearance,
            desktop_notification,
            setup_script,
        } = self;
        let mut command_launch = command_launch.into_provider();
        let mut resource_reveal = resource_reveal.into_provider();
        let mut url_open = url_open.into_provider();
        let mut desktop_appearance = desktop_appearance.into_provider();
        let mut executors = IntegrationExecutors::new(
            move |command| command_launch.run_command(&command),
            move |target, cached_executable| {
                resource_reveal.reveal_process(&target, cached_executable.as_deref())
            },
            move |url| url_open.open_url(&url),
            move || desktop_appearance.observe(),
        );
        if let Some(desktop_notification) = desktop_notification {
            let mut desktop_notification = desktop_notification.into_provider();
            executors = executors.with_desktop_notification(move |request| {
                desktop_notification.notify(
                    &request.title,
                    &request.body,
                    request.severity,
                    &request.target,
                )
            });
        }
        match setup_script {
            Some(setup_script) => {
                let mut setup_script = setup_script.into_provider();
                executors.with_setup_script(move |action| setup_script.perform(action))
            }
            None => executors,
        }
    }
}
