//! Linux desktop appearance observation.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use taskmanager_core::{
    DesktopAppearance, DesktopFamily, FailureKind, PreferredColorScheme, ProviderId, SourceOutcome,
    SourceStatus,
};
use taskmanager_platform_contract::CompositeSourceSnapshot;

use taskmanager_platform_portable::{BoundedCommandError, run_with_timeout};

const SETTINGS_TIMEOUT: Duration = Duration::from_millis(250);

pub(crate) fn observe_desktop_appearance() -> CompositeSourceSnapshot<DesktopAppearance> {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .or_else(|_| std::env::var("XDG_SESSION_DESKTOP"))
        .ok();
    let family = desktop
        .as_deref()
        .map(parse_desktop_family)
        .unwrap_or_default();
    let family_status = value_status(
        "linux.desktop.session",
        (!matches!(family, DesktopFamily::Unknown)).then_some(()),
    );

    let gsettings_scheme = gsettings_value(
        "org.gnome.desktop.interface",
        "color-scheme",
        "linux.desktop.gsettings.color-scheme",
    )
    .and_then(|value| parse_color_scheme(&value));
    let gtk_scheme = std::env::var("GTK_THEME")
        .ok()
        .and_then(|value| parse_color_scheme(&value));
    let gtk_status = value_status("linux.desktop.gtk-theme", gtk_scheme);
    let (kde_scheme, kde_status) = kde_color_scheme();

    let color_scheme = gsettings_scheme
        .value
        .or(gtk_scheme)
        .or(kde_scheme)
        .unwrap_or_default();

    let high_contrast = gsettings_value(
        "org.gnome.desktop.a11y.interface",
        "high-contrast",
        "linux.desktop.gsettings.high-contrast",
    )
    .and_then(|value| parse_bool(&value));

    CompositeSourceSnapshot::new(
        DesktopAppearance {
            family,
            color_scheme,
            high_contrast: high_contrast.value,
        },
        vec![
            family_status,
            gsettings_scheme.status,
            gtk_status,
            kde_status,
            high_contrast.status,
        ],
    )
}

struct Probe<T> {
    value: Option<T>,
    status: SourceStatus,
}

impl<T> Probe<T> {
    fn and_then<U>(self, map: impl FnOnce(T) -> Option<U>) -> Probe<U> {
        let value = self.value.and_then(map);
        let mut status = self.status;
        if value.is_none() && matches!(status.outcome, SourceOutcome::Available) {
            status.outcome = SourceOutcome::Partial(FailureKind::ProviderFault);
            status.item_count = 0;
        }
        Probe { value, status }
    }
}

fn gsettings_value(schema: &str, key: &str, provider: &'static str) -> Probe<String> {
    let mut command = Command::new("gsettings");
    command.args(["get", schema, key]);
    match run_with_timeout(&mut command, SETTINGS_TIMEOUT) {
        Ok(output) if output.status.success() => {
            let value = String::from_utf8_lossy(&output.stdout)
                .trim()
                .trim_matches('\'')
                .to_owned();
            let value = (!value.is_empty()).then_some(value);
            Probe {
                status: value_status(provider, value.as_ref()),
                value,
            }
        }
        Ok(_) => Probe {
            value: None,
            status: unavailable_status(provider, FailureKind::Unsupported),
        },
        Err(error) => Probe {
            value: None,
            status: unavailable_status(provider, command_failure(error)),
        },
    }
}

fn kde_color_scheme() -> (Option<PreferredColorScheme>, SourceStatus) {
    let Some(home) = std::env::var_os("HOME") else {
        return (None, value_status::<()>("linux.desktop.kdeglobals", None));
    };
    let path = PathBuf::from(home).join(".config/kdeglobals");
    match fs::read_to_string(path) {
        Ok(contents) => {
            let scheme = parse_kde_color_scheme(&contents);
            (scheme, value_status("linux.desktop.kdeglobals", scheme))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            (None, value_status::<()>("linux.desktop.kdeglobals", None))
        }
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => (
            None,
            unavailable_status("linux.desktop.kdeglobals", FailureKind::PermissionDenied),
        ),
        Err(_) => (
            None,
            unavailable_status("linux.desktop.kdeglobals", FailureKind::ProviderFault),
        ),
    }
}

fn parse_desktop_family(value: &str) -> DesktopFamily {
    for segment in value.to_ascii_lowercase().split([':', ';']) {
        let desktop = segment.trim();
        if desktop.contains("kde") || desktop.contains("plasma") {
            return DesktopFamily::Kde;
        }
        if ["gnome", "ubuntu", "pop", "unity", "cinnamon", "pantheon"]
            .iter()
            .any(|candidate| desktop.contains(candidate))
        {
            return DesktopFamily::Gnome;
        }
    }
    DesktopFamily::Unknown
}

fn parse_color_scheme(value: &str) -> Option<PreferredColorScheme> {
    let value = value.to_ascii_lowercase();
    if value.contains("dark") || value.contains("night") {
        Some(PreferredColorScheme::Dark)
    } else if value.contains("light") || value.contains("default") {
        Some(PreferredColorScheme::Light)
    } else {
        None
    }
}

fn parse_kde_color_scheme(contents: &str) -> Option<PreferredColorScheme> {
    contents
        .lines()
        .find_map(|line| line.trim().strip_prefix("ColorScheme="))
        .and_then(parse_color_scheme)
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

fn value_status<T>(provider: &'static str, value: Option<T>) -> SourceStatus {
    SourceStatus {
        provider: ProviderId::borrowed(provider),
        outcome: if value.is_some() {
            SourceOutcome::Available
        } else {
            SourceOutcome::Empty
        },
        item_count: usize::from(value.is_some()),
    }
}

fn unavailable_status(provider: &'static str, failure: FailureKind) -> SourceStatus {
    SourceStatus {
        provider: ProviderId::borrowed(provider),
        outcome: SourceOutcome::Unavailable(failure),
        item_count: 0,
    }
}

fn command_failure(error: BoundedCommandError) -> FailureKind {
    match error {
        BoundedCommandError::Spawn(error)
            if error.kind() == std::io::ErrorKind::PermissionDenied =>
        {
            FailureKind::PermissionDenied
        }
        BoundedCommandError::Spawn(error) if error.kind() == std::io::ErrorKind::NotFound => {
            FailureKind::MissingDependency
        }
        BoundedCommandError::Spawn(_)
        | BoundedCommandError::ReaderStart(_)
        | BoundedCommandError::ReaderFailed
        | BoundedCommandError::ProcessTree
        | BoundedCommandError::OutputTooLarge => FailureKind::ProviderFault,
        BoundedCommandError::TimedOut | BoundedCommandError::ReaderTimedOut => {
            FailureKind::TimedOut
        }
    }
}

#[cfg(test)]
#[path = "../../tests/headless/linux_engine_desktop_appearance_tests.rs"]
mod tests;
