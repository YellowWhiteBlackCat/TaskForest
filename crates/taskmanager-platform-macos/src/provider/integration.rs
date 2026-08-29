//! macOS integration-domain providers built exclusively on safe wrappers and
//! bounded `std::process` shell-outs to macOS system tools (ADR-019).
//!
//! URL open uses the `open` crate; command launch uses `sh -c` (same shape as
//! the Linux adapter); resource reveal uses `open -R` (reveal in Finder);
//! desktop appearance reads `defaults` (AppleInterfaceStyle /
//! AppleIncreaseContrast). The optional desktop-notification and first-run
//! setup facets have no safe macOS route yet: they register pending providers
//! so the capabilities keep an honest catalog descriptor typed `Unsupported`
//! instead of being absent from enumeration.

use std::path::Path;
use std::time::Duration;

use taskmanager_application::{
    CommandLaunchRequest, DesktopAppearanceRequest, DesktopNotificationRequest,
    ResourceRevealRequest, SetupScriptRequest, UrlOpenRequest,
};
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_core::{
    AlertSeverity, DesktopAppearance, DesktopFamily, FailureKind, FrozenProcessIdentity,
    PreferredColorScheme, ProviderId, SetupScriptAction, SetupScriptEvent,
};
use taskmanager_platform_contract::{CompositeSourceSnapshot, ProviderFailure};
use taskmanager_platform_provider::{
    CommandLaunchProvider, DesktopAppearanceProvider, DesktopNotificationProvider,
    ResourceRevealProvider, SetupScriptProvider, UrlOpenProvider,
};
use taskmanager_platform_runtime::{
    IntegrationExecutors, IntegrationProviderBindings, ProviderRegistration,
};

use taskmanager_platform_portable::run_with_timeout;

const DESKTOP_APPEARANCE_PROVIDER: ProviderId = ProviderId::borrowed("macos.desktop.appearance");

fn classify_shell_error(error: &str) -> ProviderFailure {
    let lower = error.to_ascii_lowercase();
    if lower.contains("permission denied") {
        ProviderFailure::PermissionDenied
    } else if lower.contains("not supported") || lower.contains("not found") {
        ProviderFailure::TemporarilyUnavailable
    } else {
        ProviderFailure::Rejected
    }
}

/// Launch a command through `sh -c` and return the child PID — the same shape
/// as the Linux adapter's `ProcessManager::run`.
pub struct MacCommandLaunchProvider;

impl CommandLaunchProvider for MacCommandLaunchProvider {
    fn run_command(&mut self, command: &str) -> Result<u32, ProviderFailure> {
        let child = std::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .spawn()
            .map_err(|error| classify_shell_error(&error.to_string()))?;
        Ok(child.id())
    }
}

/// Reveal a file in Finder through `open -R` (safe `std` shell-out).
pub struct MacResourceRevealProvider;

impl ResourceRevealProvider for MacResourceRevealProvider {
    fn reveal_process(
        &mut self,
        target: &FrozenProcessIdentity,
        cached_executable: Option<&Path>,
    ) -> Result<(), ProviderFailure> {
        // The cached path is not process identity. macOS currently exposes no
        // safe precise creation-token validation seam in this adapter, so do
        // not launch Finder for a PID that may already name a replacement.
        let _ = (target, cached_executable);
        Err(ProviderFailure::Unsupported)
    }
}

/// Open a URL through the platform shell (safe `open` crate).
pub struct MacUrlOpenProvider;

impl UrlOpenProvider for MacUrlOpenProvider {
    fn open_url(&mut self, url: &str) -> Result<(), ProviderFailure> {
        open::that(url).map_err(|_| ProviderFailure::ProviderFault)
    }
}

/// Desktop appearance from `defaults` (safe shell-out): `AppleInterfaceStyle`
/// confirms the dark scheme; `AppleIncreaseContrast` confirms high contrast.
/// Unread defaults stay Unknown/None — never guessed.
pub struct MacDesktopAppearanceProvider;

fn defaults_read(key: &str) -> Option<String> {
    let mut command = std::process::Command::new("defaults");
    command.args(["read", "-g", key]);
    match run_with_timeout(&mut command, Duration::from_secs(2)) {
        Ok(output) if output.status.success() => {
            Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
        }
        _ => None,
    }
}

impl DesktopAppearanceProvider for MacDesktopAppearanceProvider {
    fn observe(&mut self) -> Result<CompositeSourceSnapshot<DesktopAppearance>, ProviderFailure> {
        let mut scheme = PreferredColorScheme::Unknown;
        let mut scheme_observed = false;
        if let Some(style) = defaults_read("AppleInterfaceStyle") {
            // `defaults read -g AppleInterfaceStyle` prints "Dark" when dark
            // mode is active; the key is absent in light mode.
            if style.trim() == "Dark" {
                scheme = PreferredColorScheme::Dark;
                scheme_observed = true;
            } else {
                scheme = PreferredColorScheme::Light;
                scheme_observed = true;
            }
        }
        let high_contrast = defaults_read("AppleIncreaseContrast").map(|value| value.trim() == "1");

        let outcome = if scheme_observed {
            SourceOutcome::Available
        } else {
            SourceOutcome::Unavailable(FailureKind::TemporarilyUnavailable)
        };
        Ok(CompositeSourceSnapshot::new(
            DesktopAppearance {
                family: DesktopFamily::Macos,
                color_scheme: scheme,
                high_contrast,
            },
            vec![SourceStatus {
                provider: DESKTOP_APPEARANCE_PROVIDER,
                outcome,
                item_count: 1,
            }],
        ))
    }
}

/// No safe macOS desktop-notification route yet: the freedesktop DBus path
/// used by Linux does not exist here, and no published Safe wrapper exposes
/// NSUserNotification / UNUserNotificationCenter. The pending provider keeps
/// the `alerts.notify` facet enumerated with a typed `Unsupported` outcome
/// (ADR-019) — a future `osascript -e 'display notification'` bounded
/// shell-out is the natural safe route.
pub struct PendingDesktopNotificationProvider;

impl DesktopNotificationProvider for PendingDesktopNotificationProvider {
    fn notify(
        &mut self,
        _title: &str,
        _body: &str,
        _severity: AlertSeverity,
        _target: &str,
    ) -> Result<(), ProviderFailure> {
        Err(ProviderFailure::Unsupported)
    }
}

/// No macOS first-run setup asset or escalation helper is packaged yet
/// (Linux ships the fixed `setup.sh` + pkexec helper pair). The pending
/// provider keeps the `first-run.setup` facet enumerated with a typed
/// `Unsupported` outcome instead of an absent descriptor (ADR-019).
pub struct PendingSetupScriptProvider;

impl SetupScriptProvider for PendingSetupScriptProvider {
    fn perform(&mut self, _action: SetupScriptAction) -> Result<SetupScriptEvent, ProviderFailure> {
        Err(ProviderFailure::Unsupported)
    }
}

pub struct MacIntegrationProviders {
    command_launch: ProviderRegistration<CommandLaunchRequest, Box<dyn CommandLaunchProvider>>,
    resource_reveal: ProviderRegistration<ResourceRevealRequest, Box<dyn ResourceRevealProvider>>,
    url_open: ProviderRegistration<UrlOpenRequest, Box<dyn UrlOpenProvider>>,
    desktop_appearance:
        ProviderRegistration<DesktopAppearanceRequest, Box<dyn DesktopAppearanceProvider>>,
    desktop_notification: Option<
        ProviderRegistration<DesktopNotificationRequest, Box<dyn DesktopNotificationProvider>>,
    >,
    setup_script: Option<ProviderRegistration<SetupScriptRequest, Box<dyn SetupScriptProvider>>>,
}

impl MacIntegrationProviders {
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

    /// Attach the optional desktop-notification facet (the same builder shape
    /// the Linux adapter uses for its real provider).
    #[must_use]
    pub fn with_desktop_notification<N>(
        mut self,
        desktop_notification: ProviderRegistration<DesktopNotificationRequest, N>,
    ) -> Self
    where
        N: DesktopNotificationProvider,
    {
        self.desktop_notification =
            Some(desktop_notification.map_provider(|provider| {
                Box::new(provider) as Box<dyn DesktopNotificationProvider>
            }));
        self
    }

    /// Attach the optional first-run setup facet (the same builder shape the
    /// Linux adapter uses for its real provider).
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

#[cfg(test)]
#[path = "../../tests/headless/macos_provider_integration.rs"]
mod tests;
