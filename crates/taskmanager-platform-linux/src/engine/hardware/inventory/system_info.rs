//! Bounded host/session facts used by the Linux hardware inventory.

use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use sysinfo::System;
use taskmanager_platform_portable::run_with_timeout;

const PACKAGE_MANAGER_TIMEOUT: Duration = Duration::from_millis(250);
const PACKAGE_DATABASE_MAX_BYTES: u64 = 64 * 1024 * 1024;
const PACMAN_PACKAGE_ENTRY_MAX: u64 = 100_000;
const PACKAGE_ENTRY_SCAN_MAX: usize = 100_000;

const KDE_DESKTOP_PACKAGES: &[&str] = &[
    "plasma-desktop",
    "plasma-workspace",
    "plasma-workspace-wayland",
];
const KWIN_PACKAGES: &[&str] = &["kwin", "kwin-wayland", "kwin-x11"];

pub(super) fn normalize_optional_text(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

/// Normalize the session-provided desktop token without launching a shell or
/// reading a second process' configuration. `XDG_CURRENT_DESKTOP` may contain
/// a colon-separated preference list; the first non-empty token is the active
/// desktop identifier exposed by the session.
pub(super) fn normalize_desktop_environment(value: String) -> Option<String> {
    value
        .split(':')
        .map(str::trim)
        .find(|token| !token.is_empty())
        .map(str::to_owned)
}

pub(super) fn normalize_virtual_terminal(value: String) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if value.starts_with("tty") {
        Some(value.to_owned())
    } else if value.chars().all(|character| character.is_ascii_digit()) {
        Some(format!("tty{value}"))
    } else {
        None
    }
}

pub(super) fn package_manager_candidates(
    distribution_id: &str,
) -> &'static [(&'static str, &'static str)] {
    const DEBIAN: &[(&str, &str)] = &[("apt", "--version"), ("apt-get", "--version")];
    const FEDORA: &[(&str, &str)] = &[
        ("dnf", "--version"),
        ("yum", "--version"),
        ("rpm", "--version"),
    ];
    const ARCH: &[(&str, &str)] = &[("pacman", "--version")];
    const SUSE: &[(&str, &str)] = &[("zypper", "--version"), ("rpm", "--version")];
    const ALPINE: &[(&str, &str)] = &[("apk", "--version")];
    const NIX: &[(&str, &str)] = &[("nix", "--version")];
    const FALLBACK: &[(&str, &str)] = &[
        ("apt", "--version"),
        ("dnf", "--version"),
        ("pacman", "--version"),
        ("zypper", "--version"),
        ("apk", "--version"),
        ("nix", "--version"),
        ("rpm", "--version"),
    ];

    let distribution_id = distribution_id.to_ascii_lowercase();
    if ["debian", "ubuntu", "linuxmint", "pop", "elementary"]
        .iter()
        .any(|value| distribution_id == *value)
    {
        DEBIAN
    } else if ["fedora", "rhel", "centos", "rocky", "almalinux"]
        .iter()
        .any(|value| distribution_id == *value)
    {
        FEDORA
    } else if ["arch", "manjaro", "endeavouros"]
        .iter()
        .any(|value| distribution_id == *value)
    {
        ARCH
    } else if ["opensuse", "opensuse-leap", "opensuse-tumbleweed"]
        .iter()
        .any(|value| distribution_id == *value)
    {
        SUSE
    } else if distribution_id == "alpine" {
        ALPINE
    } else if distribution_id == "nixos" {
        NIX
    } else {
        FALLBACK
    }
}

pub(super) fn parse_version_token(output: &str) -> Option<String> {
    output
        .lines()
        .flat_map(str::split_whitespace)
        .find_map(|token| {
            let token = token.trim_matches(|character: char| {
                !character.is_ascii_alphanumeric()
                    && !matches!(character, '.' | '-' | '_' | '+' | ':')
            });
            let token = token.strip_prefix('v').unwrap_or(token);
            (token.chars().any(|character| character.is_ascii_digit())
                && token.chars().any(|character| character == '.'))
            .then(|| token.to_owned())
        })
}

pub(super) fn detect_package_manager(distribution_id: &str) -> (Option<String>, Option<String>) {
    for (name, argument) in package_manager_candidates(distribution_id) {
        let mut command = Command::new(name);
        command.arg(argument);
        let Ok(output) = run_with_timeout(&mut command, PACKAGE_MANAGER_TIMEOUT) else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let version = parse_version_token(&text)
            .or_else(|| parse_version_token(&String::from_utf8_lossy(&output.stderr)));
        return (Some((*name).to_owned()), version);
    }
    (None, None)
}

/// Count installed packages from the package database already present on the
/// host. This deliberately avoids `pacman -Q`, `dpkg-query`, or another
/// package-manager process: the database is a cheaper, read-only source and
/// keeps inventory collection bounded and shell-free.
pub(super) fn detect_package_count(package_manager: Option<&str>) -> Option<u64> {
    detect_package_count_at(
        package_manager,
        Path::new("/var/lib/pacman/local"),
        Path::new("/var/lib/dpkg/status"),
        Path::new("/lib/apk/db/installed"),
    )
}

/// Testable package-count implementation with injected database paths.
pub(super) fn detect_package_count_at(
    package_manager: Option<&str>,
    pacman_root: &Path,
    dpkg_status: &Path,
    apk_status: &Path,
) -> Option<u64> {
    match package_manager?.trim().to_ascii_lowercase().as_str() {
        "pacman" => count_pacman_entries(pacman_root),
        "apt" | "apt-get" => count_dpkg_installed(dpkg_status),
        "apk" => count_apk_installed(apk_status),
        _ => None,
    }
}

fn count_pacman_entries(root: &Path) -> Option<u64> {
    let entries = fs::read_dir(root).ok()?;
    let mut count = 0_u64;
    for entry in entries {
        let entry = entry.ok()?;
        if !entry.file_type().ok()?.is_dir() {
            continue;
        }
        count = count.saturating_add(1);
        if count > PACMAN_PACKAGE_ENTRY_MAX {
            return None;
        }
    }
    Some(count)
}

fn bounded_text(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(PACKAGE_DATABASE_MAX_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() as u64 <= PACKAGE_DATABASE_MAX_BYTES).then(|| String::from_utf8(bytes).ok())?
}

fn count_dpkg_installed(path: &Path) -> Option<u64> {
    let text = bounded_text(path)?;
    Some(
        text.split("\n\n")
            .filter(|stanza| {
                let has_package = stanza.lines().any(|line| {
                    line.strip_prefix("Package:")
                        .is_some_and(|name| !name.trim().is_empty())
                });
                let installed = stanza.lines().find_map(|line| {
                    line.strip_prefix("Status:")
                        .map(|status| status.split_whitespace().nth(2) == Some("installed"))
                });
                has_package && installed == Some(true)
            })
            .count() as u64,
    )
}

fn count_apk_installed(path: &Path) -> Option<u64> {
    let text = bounded_text(path)?;
    Some(
        text.lines()
            .filter(|line| line.strip_prefix("P:").is_some_and(|name| !name.is_empty()))
            .count() as u64,
    )
}

/// Read a desktop version from the package database, never by launching the
/// desktop shell. The result is a package version and is only used for an
/// active KDE session.
pub(super) fn detect_desktop_environment_version(
    desktop: &str,
    package_manager: Option<&str>,
) -> Option<String> {
    let is_kde = desktop
        .split(':')
        .map(str::trim)
        .any(|token| token.eq_ignore_ascii_case("kde") || token.eq_ignore_ascii_case("plasma"));
    is_kde.then(|| detect_package_version(package_manager, KDE_DESKTOP_PACKAGES))?
}

/// Detect KWin from the running process table. `XDG_CURRENT_DESKTOP` is only a
/// desktop-session label and must not, by itself, claim a window manager.
pub(super) fn detect_window_manager(system: &System) -> (Option<String>, Option<String>) {
    let mut wayland = false;
    let mut x11 = false;
    for process in system.processes().values() {
        match process.name().to_string_lossy().as_ref() {
            "kwin_wayland" | "kwin_wayland_wrapper" => wayland = true,
            "kwin_x11" => x11 = true,
            _ => {}
        }
    }
    if wayland {
        (Some("KWin".to_owned()), Some("Wayland".to_owned()))
    } else if x11 {
        (Some("KWin".to_owned()), Some("X11".to_owned()))
    } else {
        (None, None)
    }
}

pub(super) fn detect_window_manager_version(
    window_manager: Option<&str>,
    package_manager: Option<&str>,
) -> Option<String> {
    (window_manager == Some("KWin"))
        .then(|| detect_package_version(package_manager, KWIN_PACKAGES))?
}

fn detect_package_version(package_manager: Option<&str>, package_names: &[&str]) -> Option<String> {
    detect_package_version_at(
        package_manager,
        Path::new("/var/lib/pacman/local"),
        Path::new("/var/lib/dpkg/status"),
        Path::new("/lib/apk/db/installed"),
        package_names,
    )
}

pub(super) fn detect_package_version_at(
    package_manager: Option<&str>,
    pacman_root: &Path,
    dpkg_status: &Path,
    apk_status: &Path,
    package_names: &[&str],
) -> Option<String> {
    match package_manager?.trim().to_ascii_lowercase().as_str() {
        "pacman" => pacman_package_version(pacman_root, package_names),
        "apt" | "apt-get" => dpkg_package_version(dpkg_status, package_names),
        "apk" => apk_package_version(apk_status, package_names),
        "dnf" | "yum" | "zypper" | "rpm" => rpm_package_version(package_names),
        _ => None,
    }
}

fn pacman_package_version(root: &Path, package_names: &[&str]) -> Option<String> {
    let entries = fs::read_dir(root).ok()?;
    for (index, entry) in entries.flatten().enumerate() {
        if index >= PACKAGE_ENTRY_SCAN_MAX {
            break;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let Some(text) = bounded_text(&entry.path().join("desc")) else {
            continue;
        };
        let mut name = None;
        let mut version = None;
        let mut lines = text.lines();
        while let Some(line) = lines.next() {
            match line {
                "%NAME%" => name = lines.next().map(str::trim),
                "%VERSION%" => version = lines.next().map(str::trim),
                _ => {}
            }
        }
        if name.is_some_and(|value| package_names.contains(&value)) {
            return version
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
        }
    }
    None
}

fn dpkg_package_version(path: &Path, package_names: &[&str]) -> Option<String> {
    let text = bounded_text(path)?;
    text.split("\n\n").find_map(|stanza| {
        let name = stanza
            .lines()
            .find_map(|line| line.strip_prefix("Package:").map(str::trim))?;
        if !package_names.contains(&name) {
            return None;
        }
        stanza
            .lines()
            .find_map(|line| line.strip_prefix("Version:").map(str::trim))
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn apk_package_version(path: &Path, package_names: &[&str]) -> Option<String> {
    let text = bounded_text(path)?;
    let mut name = None;
    let mut version = None;
    for line in text.lines().chain(std::iter::once("")) {
        if let Some(value) = line.strip_prefix("P:") {
            name = Some(value.trim());
        } else if let Some(value) = line.strip_prefix("V:") {
            version = Some(value.trim());
        } else if line.trim().is_empty() {
            if name.is_some_and(|value| package_names.contains(&value)) {
                return version
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned);
            }
            name = None;
            version = None;
        }
    }
    None
}

fn rpm_package_version(package_names: &[&str]) -> Option<String> {
    for package_name in package_names {
        let mut command = Command::new("rpm");
        command.args(["-q", "--qf", "%{VERSION}-%{RELEASE}\\n", package_name]);
        let Ok(output) = run_with_timeout(&mut command, PACKAGE_MANAGER_TIMEOUT) else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        if let Some(version) = parse_rpm_package_version(&String::from_utf8_lossy(&output.stdout)) {
            return Some(version);
        }
    }
    None
}

pub(super) fn parse_rpm_package_version(output: &str) -> Option<String> {
    let value = output.trim();
    (!value.is_empty()
        && !value.contains(['\n', '\r'])
        && value.len() <= 256
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, '.' | '-' | '_' | '+' | ':' | '~' | '^')
        }))
    .then(|| value.to_owned())
}
