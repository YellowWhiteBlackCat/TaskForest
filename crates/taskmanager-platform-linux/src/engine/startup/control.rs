//! Final authorization and mutation of native startup targets.

use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
#[cfg(target_family = "unix")]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(target_family = "unix")]
use nix::fcntl::OFlag;
use taskmanager_core::core::startup::{
    StartupControlPolicy, StartupEntry, StartupScope, StartupSource,
};
use taskmanager_platform_contract::ProviderFailure;
use tracing::{info, warn};

use super::{
    StartupManager, autostart_dirs, native_startup_id, parse_desktop_entry, user_autostart_dir,
};

static OVERRIDE_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

impl StartupManager {
    /// Enable or disable an entry after resolving its provider-native locator
    /// again at the final command boundary.
    ///
    /// A shared `StartupEntryLocator` is only an opaque address. Possession of
    /// one never authorizes an arbitrary file or native command target.
    pub fn set_enabled(&self, entry: &StartupEntry, enabled: bool) -> Result<(), ProviderFailure> {
        let current_id = native_startup_id(entry.source, entry.locator.as_str());
        if entry.id.is_empty() || entry.id != current_id {
            return Err(ProviderFailure::IdentityChanged);
        }
        match entry.source {
            StartupSource::DesktopEntry
                if entry.scope == StartupScope::User
                    && entry.control_policy == StartupControlPolicy::Direct =>
            {
                set_desktop_entry_enabled(entry, enabled)
            }
            StartupSource::DesktopEntry
                if entry.scope == StartupScope::System
                    && entry.control_policy == StartupControlPolicy::UserOverride =>
            {
                set_system_desktop_override(entry, enabled)
            }
            StartupSource::UserService
                if entry.scope == StartupScope::User
                    && entry.control_policy == StartupControlPolicy::Direct =>
            {
                self.set_init_source_enabled(entry, enabled)
            }
            StartupSource::RunLevel
                if entry.scope == StartupScope::System
                    && entry.control_policy == StartupControlPolicy::Direct =>
            {
                self.set_init_source_enabled(entry, enabled)
            }
            StartupSource::DesktopEntry | StartupSource::UserService | StartupSource::RunLevel => {
                Err(ProviderFailure::Rejected)
            }
            StartupSource::SystemService
            | StartupSource::RegistryEntry
            | StartupSource::ScheduledTask
            | StartupSource::LoginItem
            | StartupSource::StartupFolder
            | StartupSource::Other => Err(ProviderFailure::Unsupported),
        }
    }
}

fn set_desktop_entry_enabled(entry: &StartupEntry, enabled: bool) -> Result<(), ProviderFailure> {
    #[cfg(not(target_family = "unix"))]
    {
        let _ = (entry, enabled);
        return Err(ProviderFailure::Unsupported);
    }

    #[cfg(target_family = "unix")]
    {
        let path = PathBuf::from(entry.locator.as_str());
        let user_root = user_autostart_dir().ok_or(ProviderFailure::Rejected)?;
        validate_desktop_locator(&path, &user_root)?;

        let metadata = fs::symlink_metadata(&path).map_err(classify_target_io)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(ProviderFailure::Rejected);
        }

        let canonical_root = fs::canonicalize(&user_root).map_err(classify_target_io)?;
        let parent = path.parent().ok_or(ProviderFailure::Rejected)?;
        let canonical_parent = fs::canonicalize(parent).map_err(classify_target_io)?;
        if canonical_parent != canonical_root {
            return Err(ProviderFailure::Rejected);
        }

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(OFlag::O_NOFOLLOW.bits() | OFlag::O_CLOEXEC.bits())
            .open(&path)
            .map_err(classify_target_io)?;
        if !file.metadata().map_err(classify_target_io)?.is_file() {
            return Err(ProviderFailure::Rejected);
        }

        let mut current_text = String::new();
        file.read_to_string(&mut current_text)
            .map_err(classify_target_io)?;
        validate_desktop_identity(entry, &current_text)?;

        let new_text = rewrite_with_hidden(&current_text, !enabled);
        file.seek(SeekFrom::Start(0)).map_err(classify_target_io)?;
        file.set_len(0).map_err(classify_target_io)?;
        file.write_all(new_text.as_bytes())
            .map_err(classify_target_io)?;
        file.sync_all().map_err(classify_target_io)?;
        info!("startup {} -> enabled={}", entry.name, enabled);
        Ok(())
    }
}

/// Install a user-scoped XDG override for a system-scoped desktop entry.
///
/// The system file is opened read-only and revalidated. The provider never
/// mutates it. A new user file with the same desktop ID is written through an
/// exclusive temporary file and installed with a no-replace hard link, so a
/// concurrently created override is not overwritten.
fn set_system_desktop_override(entry: &StartupEntry, enabled: bool) -> Result<(), ProviderFailure> {
    #[cfg(not(target_family = "unix"))]
    {
        let _ = (entry, enabled);
        return Err(ProviderFailure::Unsupported);
    }

    #[cfg(target_family = "unix")]
    {
        let source_path = PathBuf::from(entry.locator.as_str());
        let user_root = user_autostart_dir().ok_or(ProviderFailure::Rejected)?;
        let system_roots = system_autostart_dirs(&user_root);
        validate_system_desktop_locator(&source_path, &system_roots)?;
        let desktop_id = source_path.file_name().ok_or(ProviderFailure::Rejected)?;
        let authoritative_source = resolve_system_desktop_source(&system_roots, desktop_id)?;
        if authoritative_source != source_path {
            return Err(ProviderFailure::IdentityChanged);
        }
        validate_regular_file_parent(&source_path)?;

        let mut source = OpenOptions::new()
            .read(true)
            .custom_flags(OFlag::O_NOFOLLOW.bits() | OFlag::O_CLOEXEC.bits())
            .open(&source_path)
            .map_err(classify_target_io)?;
        if !source.metadata().map_err(classify_target_io)?.is_file() {
            return Err(ProviderFailure::Rejected);
        }
        let mut current_text = String::new();
        source
            .read_to_string(&mut current_text)
            .map_err(classify_target_io)?;
        validate_desktop_identity(entry, &current_text)?;

        let override_text = rewrite_with_hidden(&current_text, !enabled);
        install_user_override(&user_root, desktop_id, &override_text)?;
        info!(
            "startup {} -> enabled={} through user override",
            entry.name, enabled
        );
        Ok(())
    }
}

fn system_autostart_dirs(user_root: &Path) -> Vec<PathBuf> {
    autostart_dirs()
        .into_iter()
        .filter(|candidate| candidate != user_root)
        .collect()
}

fn validate_system_desktop_locator(
    path: &Path,
    system_roots: &[PathBuf],
) -> Result<(), ProviderFailure> {
    if !path.is_absolute()
        || path.extension().and_then(|extension| extension.to_str()) != Some("desktop")
        || path.file_name().is_none()
        || !system_roots
            .iter()
            .any(|root| root.is_absolute() && path.parent().is_some_and(|parent| parent == root))
    {
        return Err(ProviderFailure::Rejected);
    }
    Ok(())
}

fn resolve_system_desktop_source(
    system_roots: &[PathBuf],
    desktop_id: &OsStr,
) -> Result<PathBuf, ProviderFailure> {
    for root in system_roots {
        let candidate = root.join(desktop_id);
        match fs::symlink_metadata(&candidate) {
            Ok(_) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(classify_target_io(error)),
        }
    }
    Err(ProviderFailure::IdentityChanged)
}

#[cfg(target_family = "unix")]
fn validate_regular_file_parent(path: &Path) -> Result<(), ProviderFailure> {
    let metadata = fs::symlink_metadata(path).map_err(classify_target_io)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ProviderFailure::Rejected);
    }
    let parent = path.parent().ok_or(ProviderFailure::Rejected)?;
    let canonical_parent = fs::canonicalize(parent).map_err(classify_target_io)?;
    if !canonical_parent.is_absolute()
        || fs::symlink_metadata(parent)
            .map_err(classify_target_io)?
            .file_type()
            .is_symlink()
    {
        return Err(ProviderFailure::Rejected);
    }
    Ok(())
}

#[cfg(target_family = "unix")]
fn install_user_override(
    user_root: &Path,
    desktop_id: &OsStr,
    text: &str,
) -> Result<(), ProviderFailure> {
    if !user_root.is_absolute() {
        return Err(ProviderFailure::Rejected);
    }
    fs::create_dir_all(user_root).map_err(classify_target_io)?;
    let root_metadata = fs::symlink_metadata(user_root).map_err(classify_target_io)?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(ProviderFailure::Rejected);
    }

    let target = user_root.join(desktop_id);
    validate_desktop_locator(&target, user_root)?;
    match fs::symlink_metadata(&target) {
        Ok(_) => return Err(ProviderFailure::IdentityChanged),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(classify_target_io(error)),
    }

    let (mut temporary, temporary_path) = create_override_temp(user_root, desktop_id)?;
    let write_result = (|| {
        temporary
            .write_all(text.as_bytes())
            .map_err(classify_target_io)?;
        temporary.sync_all().map_err(classify_target_io)?;
        drop(temporary);
        fs::hard_link(&temporary_path, &target).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                ProviderFailure::IdentityChanged
            } else {
                classify_target_io(error)
            }
        })?;
        File::open(user_root)
            .and_then(|directory| directory.sync_all())
            .map_err(classify_target_io)
    })();

    if let Err(error) = fs::remove_file(&temporary_path)
        && write_result.is_ok()
    {
        warn!(
            "startup override installed but temporary link cleanup failed: {}",
            error
        );
    }
    write_result
}

#[cfg(target_family = "unix")]
fn create_override_temp(
    user_root: &Path,
    desktop_id: &OsStr,
) -> Result<(File, PathBuf), ProviderFailure> {
    const MAX_ATTEMPTS: usize = 16;
    let desktop_id = desktop_id.to_string_lossy();
    for _ in 0..MAX_ATTEMPTS {
        let sequence = OVERRIDE_TEMP_SEQUENCE
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        let path = user_root.join(format!(
            ".{desktop_id}.taskforest-{}-{sequence}.tmp",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(OFlag::O_NOFOLLOW.bits() | OFlag::O_CLOEXEC.bits())
            .open(&path)
        {
            Ok(file) => return Ok((file, path)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(classify_target_io(error)),
        }
    }
    Err(ProviderFailure::TemporarilyUnavailable)
}

fn validate_desktop_locator(path: &Path, user_root: &Path) -> Result<(), ProviderFailure> {
    if !path.is_absolute()
        || !user_root.is_absolute()
        || path.parent() != Some(user_root)
        || path.extension().and_then(|extension| extension.to_str()) != Some("desktop")
        || path.file_name().is_none()
    {
        return Err(ProviderFailure::Rejected);
    }
    Ok(())
}

fn validate_desktop_identity(
    expected: &StartupEntry,
    current_text: &str,
) -> Result<(), ProviderFailure> {
    let current = parse_desktop_entry(current_text);
    let current_name = current.name.ok_or(ProviderFailure::IdentityChanged)?;
    let current_exec = current.exec.unwrap_or_default();
    let current_enabled = !current.hidden;
    if current_name != expected.name
        || current_exec != expected.exec
        || current_enabled != expected.enabled
    {
        return Err(ProviderFailure::IdentityChanged);
    }
    Ok(())
}

fn classify_target_io(error: std::io::Error) -> ProviderFailure {
    match error.kind() {
        std::io::ErrorKind::NotFound => ProviderFailure::IdentityChanged,
        std::io::ErrorKind::PermissionDenied => ProviderFailure::PermissionDenied,
        std::io::ErrorKind::TimedOut => ProviderFailure::TimedOut,
        std::io::ErrorKind::Unsupported => ProviderFailure::Unsupported,
        _ => ProviderFailure::ProviderFault,
    }
}

/// Toggle the `Hidden=` key in a `.desktop` file.
///
/// Re-enabling also removes `NoDisplay=true`, because discovery treats both
/// keys as disabled. This transform is pure; authorization and identity checks
/// happen before it is applied to an open target.
pub(super) fn rewrite_with_hidden(text: &str, hide: bool) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut in_entry = false;
    let mut saw_hidden = false;
    for raw in text.lines() {
        let line = raw.trim();
        if let Some(header) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            in_entry = header.eq_ignore_ascii_case("Desktop Entry");
        }
        let key = line.split_once('=').map(|(key, _)| key.trim());
        let is_hidden_key = in_entry && key.is_some_and(|key| key.eq_ignore_ascii_case("Hidden"));
        let is_nodisplay_key =
            in_entry && key.is_some_and(|key| key.eq_ignore_ascii_case("NoDisplay"));
        if is_hidden_key {
            saw_hidden = true;
            if hide {
                out.push_str("Hidden=true\n");
            }
            continue;
        }
        if !hide && is_nodisplay_key {
            continue;
        }
        out.push_str(raw);
        out.push('\n');
    }
    if hide && !saw_hidden {
        out.push_str("Hidden=true\n");
    }
    out
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_startup_control_tests.rs"]
mod tests;
