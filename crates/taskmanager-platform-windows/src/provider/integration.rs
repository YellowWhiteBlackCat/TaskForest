//! Windows integration-domain providers built on safe std shell-outs and safe
//! wrapper crates.
//!
//! Command launch uses `std::process::Command` (cmd.exe), URL open uses the
//! safe `open` crate, and desktop appearance reads the theme + high-contrast
//! accessibility registry through `windows-registry`. Reveal-in-explorer spawns
//! `explorer.exe /select,<path>` fire-and-forget (spawn seam-injected in tests
//! so no real File Explorer window opens on a Windows test host); with no
//! cached executable path it returns `TemporarilyUnavailable` (kept in the
//! pending set) — never `Unsupported` (ADR-018 route-C safe-shell-out policy).
//!
//! The optional desktop-notification and first-run setup facets have no safe
//! Windows route yet: they register pending providers so the capabilities
//! keep an honest catalog descriptor typed `Unsupported` instead of being
//! absent from enumeration (G-05).

#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::sync::Arc;

use taskmanager_application::{
    CommandLaunchRequest, DesktopAppearanceRequest, DesktopNotificationRequest,
    ResourceRevealRequest, SetupScriptRequest, UrlOpenRequest,
};
use taskmanager_core::{
    AlertSeverity, DesktopAppearance, DesktopFamily, FailureKind, FrozenProcessIdentity,
    PreferredColorScheme, ProviderId, SetupScriptAction, SetupScriptEvent,
};
use taskmanager_platform_contract::{
    CompositeSourceSnapshot, ProviderFailure, SourceOutcome, SourceStatus,
};
use taskmanager_platform_provider::{
    CommandLaunchProvider, DesktopAppearanceProvider, DesktopNotificationProvider,
    ResourceRevealProvider, SetupScriptProvider, UrlOpenProvider,
};
use taskmanager_platform_runtime::{
    IntegrationExecutors, IntegrationProviderBindings, ProviderRegistration,
};

const DESKTOP_APPEARANCE_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.desktop.appearance.registry");

/// Launch a command through `cmd /C` and return the child PID (safe `std`
/// process spawn; no shell metacharacter handling is attempted beyond what
/// cmd.exe already performs).
pub struct WinCommandLaunchProvider;

impl CommandLaunchProvider for WinCommandLaunchProvider {
    fn run_command(&mut self, command: &str) -> Result<u32, ProviderFailure> {
        let mut child_command = std::process::Command::new("cmd");
        // CREATE_NO_WINDOW: a launched command must never flash a console
        // window (the app itself is a GUI/headless process).
        #[cfg(windows)]
        child_command.creation_flags(0x0800_0000);
        let child = child_command
            .arg("/C")
            .arg(command)
            .spawn()
            .map_err(|_| ProviderFailure::PermissionDenied)?;
        Ok(child.id())
    }
}

/// Open a URL through the platform shell (safe `open` crate).
pub struct WinUrlOpenProvider;

impl UrlOpenProvider for WinUrlOpenProvider {
    fn open_url(&mut self, url: &str) -> Result<(), ProviderFailure> {
        open::that(url).map_err(|_| ProviderFailure::ProviderFault)
    }
}

/// Desktop appearance from the theme registry (safe `windows-registry`):
/// `AppsUseLightTheme`/`SystemUsesLightTheme` give the confirmed scheme, and
/// the `HighContrastOn` DWORD under `Control Panel\Accessibility\HighContrast`
/// (1 = on, anything else = off) gives the high-contrast flag. A missing key or
/// value leaves the corresponding field at its `None` default rather than
/// failing the appearance snapshot (ADR-018 route-A registry read).
pub struct WinDesktopAppearanceProvider;

/// Native Windows implementation: reads `AppsUseLightTheme` and
/// `HighContrastOn` from the theme + accessibility registry through the safe
/// `windows-registry` crate; missing values degrade gracefully to `None`.
#[cfg(windows)]
impl DesktopAppearanceProvider for WinDesktopAppearanceProvider {
    fn observe(&mut self) -> Result<CompositeSourceSnapshot<DesktopAppearance>, ProviderFailure> {
        const THEME_PERSONALIZE: &str =
            "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize";
        const HIGH_CONTRAST: &str = "Control Panel\\Accessibility\\HighContrast";
        let mut scheme = PreferredColorScheme::Unknown;
        let mut scheme_observed = false;
        if let (Ok(key), Ok(value)) = (
            windows_registry::CURRENT_USER.open(THEME_PERSONALIZE),
            windows_registry::CURRENT_USER
                .open(THEME_PERSONALIZE)
                .and_then(|key| key.get_u32("AppsUseLightTheme")),
        ) {
            let _ = key;
            scheme = if value != 0 {
                PreferredColorScheme::Light
            } else {
                PreferredColorScheme::Dark
            };
            scheme_observed = true;
        }
        // High-contrast flag: DWORD `HighContrastOn` under the Accessibility
        // key. A missing key/value falls through to `None` (no reading) and
        // never fails the appearance snapshot — mirrors the scheme read's
        // graceful-degrade shape above.
        let high_contrast_raw: Option<u32> = windows_registry::CURRENT_USER
            .open(HIGH_CONTRAST)
            .and_then(|key| key.get_u32("HighContrastOn"))
            .ok();
        let high_contrast = map_high_contrast(high_contrast_raw);
        let appearance = DesktopAppearance {
            family: DesktopFamily::Windows,
            color_scheme: scheme,
            high_contrast,
        };
        let outcome = if scheme_observed {
            SourceOutcome::Available
        } else {
            SourceOutcome::Unavailable(FailureKind::TemporarilyUnavailable)
        };
        Ok(CompositeSourceSnapshot::new(
            appearance,
            vec![SourceStatus {
                provider: DESKTOP_APPEARANCE_PROVIDER,
                outcome,
                item_count: 1,
            }],
        ))
    }
}

/// Off-Windows fallback: the theme + accessibility registry is absent, so the
/// appearance source completes honestly unavailable (`MissingDependency`)
/// rather than fabricating a scheme or a high-contrast flag — both stay at
/// their `None`/`Unknown` defaults. Keeps the adapter composable +
/// contract-testable on the Linux CI gate (mirrors the macOS adapter's
/// cross-target model).
#[cfg(not(windows))]
impl DesktopAppearanceProvider for WinDesktopAppearanceProvider {
    fn observe(&mut self) -> Result<CompositeSourceSnapshot<DesktopAppearance>, ProviderFailure> {
        Ok(CompositeSourceSnapshot::new(
            DesktopAppearance {
                family: DesktopFamily::Windows,
                color_scheme: PreferredColorScheme::Unknown,
                high_contrast: None,
            },
            vec![SourceStatus {
                provider: DESKTOP_APPEARANCE_PROVIDER,
                outcome: SourceOutcome::Unavailable(FailureKind::MissingDependency),
                item_count: 1,
            }],
        ))
    }
}

/// Map a `HighContrastOn` DWORD (read from the accessibility registry) to the
/// high-contrast boolean. `Some(1)` => on, any other present value => off, a
/// missing value => `None` (no reading). Pure mapping helper, unit-tested
/// below; gated so it compiles only where it is reachable (Windows impl or the
/// host-independent test) — on the Linux CI gate it is simply absent.
#[cfg(any(windows, test))]
fn map_high_contrast(dword: Option<u32>) -> Option<bool> {
    dword.map(|value| value == 1)
}

/// Process seam for the fire-and-forget `explorer.exe /select,<path>` spawn.
/// Production uses the real `Command`; tests inject a recorder so the provider's
/// mapping logic runs on every host without popping a real File Explorer window.
trait ExplorerSpawn {
    fn reveal(&self, select_arg: &str) -> std::io::Result<()>;
}

/// Production spawner: `explorer.exe /select,<path>` via `std::process::Command`
/// (explorer hands off to the shell window process and returns at once, so no
/// bounded wait is needed).
struct RealExplorerSpawn;

impl ExplorerSpawn for RealExplorerSpawn {
    fn reveal(&self, select_arg: &str) -> std::io::Result<()> {
        std::process::Command::new("explorer")
            .arg(select_arg)
            .spawn()
            .map(|_| ())
    }
}

/// Reveal a process executable in Windows File Explorer via a fire-and-forget
/// `explorer.exe /select,<absolute-path>` spawn (mirrors `WinCommandLaunch`'s
/// cmd spawn). With no cached executable path there is nothing to reveal — an
/// honest `TemporarilyUnavailable` (kept in the pending set for retry), not
/// `Unsupported`. The `explorer` binary is absent on the Linux CI gate, so a
/// spawn `NotFound` maps to `MissingDependency`; any other spawn failure maps
/// to `PermissionDenied`. ADR-018 route-C.
pub struct WinResourceRevealProvider {
    spawner: Arc<dyn ExplorerSpawn + Send + Sync>,
}

impl WinResourceRevealProvider {
    #[must_use]
    pub fn new() -> Self {
        Self {
            spawner: Arc::new(RealExplorerSpawn),
        }
    }

    fn reveal_with(
        &self,
        cached_executable: Option<&std::path::Path>,
    ) -> Result<(), ProviderFailure> {
        let Some(path) = cached_executable else {
            return Err(ProviderFailure::TemporarilyUnavailable);
        };
        let select_arg = format!("/select,{}", path.display());
        match self.spawner.reveal(&select_arg) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Err(ProviderFailure::MissingDependency)
            }
            Err(_) => Err(ProviderFailure::PermissionDenied),
        }
    }
}

impl Default for WinResourceRevealProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourceRevealProvider for WinResourceRevealProvider {
    fn reveal_process(
        &mut self,
        target: &FrozenProcessIdentity,
        cached_executable: Option<&std::path::Path>,
    ) -> Result<(), ProviderFailure> {
        crate::provider::process::validate_process_target(target)?;
        self.reveal_with(cached_executable)
    }
}

/// Windows desktop notification provider using the safe `notify-rust` crate
/// (backed by Windows 10/11 native WinRT Toast notifications).
pub struct WinDesktopNotificationProvider;

impl DesktopNotificationProvider for WinDesktopNotificationProvider {
    fn notify(
        &mut self,
        title: &str,
        body: &str,
        _severity: AlertSeverity,
        _target: &str,
    ) -> Result<(), ProviderFailure> {
        #[cfg(windows)]
        {
            let summary = if title.trim().is_empty() {
                "TaskForest"
            } else {
                title.trim()
            };
            if body.trim().is_empty() {
                return Ok(());
            }
            notify_rust::Notification::new()
                .summary(summary)
                .body(body.trim())
                .appname("TaskForest")
                .show()
                .map_err(|_| ProviderFailure::TemporarilyUnavailable)?;
            Ok(())
        }
        #[cfg(not(windows))]
        {
            let _ = (title, body);
            Err(ProviderFailure::Unsupported)
        }
    }
}

/// No Windows first-run setup asset or escalation helper is packaged yet
/// (Linux ships the fixed `setup.sh` + pkexec helper pair; the Windows
/// equivalent would need an elevated-prompt runner that does not exist in a
/// safe wrapper). The pending provider keeps the `first-run.setup` facet
/// enumerated with a typed `Unsupported` outcome instead of an absent
/// descriptor (ADR-018).
pub struct PendingSetupScriptProvider;

impl SetupScriptProvider for PendingSetupScriptProvider {
    fn perform(&mut self, _action: SetupScriptAction) -> Result<SetupScriptEvent, ProviderFailure> {
        Err(ProviderFailure::Unsupported)
    }
}

pub struct WinIntegrationProviders {
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

impl WinIntegrationProviders {
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
#[path = "../../tests/headless/platform_windows_provider_integration.rs"]
mod tests;
