//! Filesystem persistence for the platform-neutral configuration model.
//!
//! The executable composition edge supplies the native config path. This
//! runtime owns only bounded file I/O and JSON failure classification; it does
//! not guess Linux, macOS, or Windows directory conventions.

use std::fmt;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use atomicwrites::{AllowOverwrite, AtomicFile};
use taskmanager_core::config::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigStoreErrorKind {
    Missing,
    Read,
    Decode,
    CreateDirectory,
    Encode,
    Write,
    Backup,
    Rename,
    TooLarge,
    Lock,
}

impl ConfigStoreErrorKind {
    #[must_use]
    pub const fn stable_code(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Read => "read",
            Self::Decode => "decode",
            Self::CreateDirectory => "create_directory",
            Self::Encode => "encode",
            Self::Write => "write",
            Self::Backup => "backup",
            Self::Rename => "rename",
            Self::TooLarge => "too_large",
            Self::Lock => "lock",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigStoreError {
    kind: ConfigStoreErrorKind,
    detail: String,
}

impl ConfigStoreError {
    #[must_use]
    pub fn new(kind: ConfigStoreErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }

    #[must_use]
    pub const fn kind(&self) -> ConfigStoreErrorKind {
        self.kind
    }

    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for ConfigStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.kind.stable_code())
    }
}

impl std::error::Error for ConfigStoreError {}

/// Upper bound for one preferences document. The configuration model is a
/// bounded user-preferences object, not an unbounded data store. Refusing an
/// unexpectedly large file prevents a damaged/replaced path from becoming a
/// memory-amplification input.
pub const MAX_CONFIG_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigLoadSource {
    Primary,
    Backup,
    Default,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigLoadResult {
    config: Config,
    source: ConfigLoadSource,
    primary_error: Option<ConfigStoreErrorKind>,
    backup_error: Option<ConfigStoreErrorKind>,
}

impl ConfigLoadResult {
    #[must_use]
    pub fn config(&self) -> &Config {
        &self.config
    }

    #[must_use]
    pub fn into_config(self) -> Config {
        self.config
    }

    #[must_use]
    pub const fn source(&self) -> ConfigLoadSource {
        self.source
    }

    #[must_use]
    pub const fn primary_error(&self) -> Option<ConfigStoreErrorKind> {
        self.primary_error
    }

    #[must_use]
    pub const fn backup_error(&self) -> Option<ConfigStoreErrorKind> {
        self.backup_error
    }
}

#[derive(Debug, Default)]
struct SaveState {
    latest_reserved_generation: u64,
    /// Merge base captured when `latest_reserved_generation` was reserved.
    /// A later load through another clone must not redefine an already queued
    /// background snapshot's local changes.
    latest_reserved_base: Option<Config>,
    /// Last configuration this writer loaded or successfully submitted.
    /// Deliberately stores the local snapshot, not a disk-merged snapshot.
    base: Option<Config>,
}

/// Injected-path configuration repository.
///
/// Keeping path selection outside this type makes a second native adapter
/// choose its own convention without putting OS branches into shared UI or
/// domain code.
#[derive(Debug, Clone)]
pub struct ConfigStore {
    path: PathBuf,
    state: Arc<Mutex<SaveState>>,
}

impl PartialEq for ConfigStore {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}

impl Eq for ConfigStore {}

impl ConfigStore {
    const BACKUP_SUFFIX: &'static str = "json.bak";
    const LOCK_SUFFIX: &'static str = "json.lock";
    const LOCK_TIMEOUT: Duration = Duration::from_millis(500);

    fn backup_path(&self) -> PathBuf {
        self.path.with_extension(Self::BACKUP_SUFFIX)
    }

    fn lock_path(&self) -> PathBuf {
        self.path.with_extension(Self::LOCK_SUFFIX)
    }

    fn state(&self) -> std::sync::MutexGuard<'_, SaveState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl ConfigStore {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            state: Arc::new(Mutex::new(SaveState::default())),
        }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<Config, ConfigStoreError> {
        let mut state = self.state();
        let config = load_config_from(&self.path)?;
        state.base = Some(config.clone());
        Ok(config)
    }

    /// Load the primary file, then a last-known-good backup, and only then
    /// default. The source and both failure classes are retained so a caller
    /// can surface recovery instead of silently claiming that preferences were
    /// never configured.
    #[must_use]
    pub fn load_with_recovery(&self) -> ConfigLoadResult {
        let mut state = self.state();
        let result = match load_config_from(&self.path) {
            Ok(config) => ConfigLoadResult {
                config,
                source: ConfigLoadSource::Primary,
                primary_error: None,
                backup_error: None,
            },
            Err(primary) => match load_config_from(&self.backup_path()) {
                Ok(config) => ConfigLoadResult {
                    config,
                    source: ConfigLoadSource::Backup,
                    primary_error: Some(primary.kind()),
                    backup_error: None,
                },
                Err(backup) => ConfigLoadResult {
                    config: Config::default(),
                    source: ConfigLoadSource::Default,
                    primary_error: Some(primary.kind()),
                    backup_error: Some(backup.kind()),
                },
            },
        };
        state.base = Some(result.config.clone());
        result
    }

    /// Preserve the historical non-blocking startup policy: a missing,
    /// unreadable, or malformed file yields a safe default.
    #[must_use]
    pub fn load_or_default(&self) -> Config {
        self.load_with_recovery().into_config()
    }

    pub fn save(&self, config: &Config) -> Result<(), ConfigStoreError> {
        let generation = self.next_save_generation();
        self.save_at(config, generation)
    }

    /// Reserve a monotonically increasing in-process save generation before a
    /// snapshot is moved to a background executor. A stale snapshot can then
    /// never overwrite a newer user action when it finishes later.
    #[must_use]
    pub fn next_save_generation(&self) -> u64 {
        let generation = next_save_generation();
        let mut state = self.state();
        state.latest_reserved_generation = generation;
        state.latest_reserved_base = state.base.clone();
        generation
    }

    /// Commit a previously reserved snapshot. Older generations become a
    /// successful no-op after a newer generation has been reserved. This keeps the
    /// UI non-blocking without allowing detached background saves to roll
    /// language/font changes backward.
    pub fn save_at(&self, config: &Config, generation: u64) -> Result<(), ConfigStoreError> {
        let mut state = self.state();
        if generation < state.latest_reserved_generation {
            return Ok(());
        }
        state.latest_reserved_generation = generation;
        let reserved_base = state.latest_reserved_base.clone();
        commit_snapshot(
            &self.path,
            &self.backup_path(),
            &self.lock_path(),
            reserved_base.as_ref(),
            config,
            Self::LOCK_TIMEOUT,
        )?;
        state.base = Some(config.clone());
        Ok(())
    }

    /// Commit one explicit base-to-local top-level patch and return the exact
    /// lock-protected snapshot written to disk.
    ///
    /// This is the persistence primitive used by the background configuration
    /// coordinator. Unlike [`Self::save_at`], the merge base belongs to the
    /// submitting client rather than this store's writer history. That keeps
    /// stale clients safe: disjoint fields compose, while the last accepted
    /// patch for the same field wins in coordinator order.
    pub fn commit_patch(&self, base: &Config, local: &Config) -> Result<Config, ConfigStoreError> {
        let mut state = self.state();
        let merged = commit_snapshot(
            &self.path,
            &self.backup_path(),
            &self.lock_path(),
            Some(base),
            local,
            Self::LOCK_TIMEOUT,
        )?;
        state.base = Some(local.clone());
        Ok(merged)
    }
}

fn commit_snapshot(
    path: &Path,
    backup_path: &Path,
    lock_path: &Path,
    base: Option<&Config>,
    local: &Config,
    lock_timeout: Duration,
) -> Result<Config, ConfigStoreError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| {
            ConfigStoreError::new(ConfigStoreErrorKind::CreateDirectory, error.to_string())
        })?;
    }
    let _lock = acquire_config_lock(lock_path, lock_timeout)?;
    let current = match fs::metadata(path) {
        Ok(_) => load_config_from(path)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => base.cloned().unwrap_or_default(),
        Err(error) => {
            return Err(ConfigStoreError::new(
                ConfigStoreErrorKind::Read,
                error.to_string(),
            ));
        }
    };
    let merged = base.map_or_else(
        || Ok(local.clone()),
        |base| merge_local_changes(base, local, &current),
    )?;
    let text = serde_json::to_string_pretty(&merged)
        .map_err(|error| ConfigStoreError::new(ConfigStoreErrorKind::Encode, error.to_string()))?;
    if text.len() > MAX_CONFIG_BYTES {
        return Err(ConfigStoreError::new(
            ConfigStoreErrorKind::TooLarge,
            format!("configuration exceeds {MAX_CONFIG_BYTES} bytes"),
        ));
    }

    write_atomic(path, backup_path, text.as_bytes())?;
    Ok(merged)
}

fn acquire_config_lock(path: &Path, timeout: Duration) -> Result<File, ConfigStoreError> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|error| ConfigStoreError::new(ConfigStoreErrorKind::Lock, error.to_string()))?;
    let deadline = Instant::now().checked_add(timeout);
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(TryLockError::WouldBlock) => {
                if deadline.is_none_or(|deadline| Instant::now() >= deadline) {
                    return Err(ConfigStoreError::new(
                        ConfigStoreErrorKind::Lock,
                        "timed out waiting for the configuration writer lock",
                    ));
                }
                std::thread::park_timeout(Duration::from_millis(2));
            }
            Err(TryLockError::Error(error)) => {
                return Err(ConfigStoreError::new(
                    ConfigStoreErrorKind::Lock,
                    error.to_string(),
                ));
            }
        }
    }
}

fn merge_local_changes(
    base: &Config,
    local: &Config,
    current: &Config,
) -> Result<Config, ConfigStoreError> {
    let encode = |config: &Config| {
        serde_json::to_value(config)
            .map_err(|error| ConfigStoreError::new(ConfigStoreErrorKind::Encode, error.to_string()))
    };
    let serde_json::Value::Object(base) = encode(base)? else {
        return Err(ConfigStoreError::new(
            ConfigStoreErrorKind::Encode,
            "configuration base is not a JSON object",
        ));
    };
    let serde_json::Value::Object(local) = encode(local)? else {
        return Err(ConfigStoreError::new(
            ConfigStoreErrorKind::Encode,
            "local configuration is not a JSON object",
        ));
    };
    let serde_json::Value::Object(mut merged) = encode(current)? else {
        return Err(ConfigStoreError::new(
            ConfigStoreErrorKind::Encode,
            "current configuration is not a JSON object",
        ));
    };
    for (field, local_value) in local {
        if base.get(&field) != Some(&local_value) {
            merged.insert(field, local_value);
        }
    }
    serde_json::from_value(serde_json::Value::Object(merged))
        .map_err(|error| ConfigStoreError::new(ConfigStoreErrorKind::Encode, error.to_string()))
}

fn next_save_generation() -> u64 {
    static NEXT_GENERATION: AtomicU64 = AtomicU64::new(0);
    NEXT_GENERATION
        .fetch_add(1, Ordering::Relaxed)
        .saturating_add(1)
}

fn load_config_from(path: &Path) -> Result<Config, ConfigStoreError> {
    let text = read_bounded_text(path)?;
    serde_json::from_str(&text)
        .map_err(|error| ConfigStoreError::new(ConfigStoreErrorKind::Decode, error.to_string()))
}

fn read_bounded_text(path: &Path) -> Result<String, ConfigStoreError> {
    let bytes = read_bounded_bytes(path)?;
    String::from_utf8(bytes)
        .map_err(|error| ConfigStoreError::new(ConfigStoreErrorKind::Decode, error.to_string()))
}

fn read_bounded_bytes(path: &Path) -> Result<Vec<u8>, ConfigStoreError> {
    let file = File::open(path).map_err(|error| {
        ConfigStoreError::new(
            if error.kind() == io::ErrorKind::NotFound {
                ConfigStoreErrorKind::Missing
            } else {
                ConfigStoreErrorKind::Read
            },
            error.to_string(),
        )
    })?;
    let size = file
        .metadata()
        .map_err(|error| ConfigStoreError::new(ConfigStoreErrorKind::Read, error.to_string()))?
        .len();
    if size > MAX_CONFIG_BYTES as u64 {
        return Err(ConfigStoreError::new(
            ConfigStoreErrorKind::TooLarge,
            format!("configuration exceeds {MAX_CONFIG_BYTES} bytes"),
        ));
    }
    let mut bytes = Vec::with_capacity(size as usize);
    file.take((MAX_CONFIG_BYTES as u64).saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| ConfigStoreError::new(ConfigStoreErrorKind::Read, error.to_string()))?;
    if bytes.len() > MAX_CONFIG_BYTES {
        return Err(ConfigStoreError::new(
            ConfigStoreErrorKind::TooLarge,
            format!("configuration exceeds {MAX_CONFIG_BYTES} bytes"),
        ));
    }
    Ok(bytes)
}

fn write_atomic(path: &Path, backup: &Path, bytes: &[u8]) -> Result<(), ConfigStoreError> {
    if path.is_file() {
        let current_size = match fs::metadata(path).map(|metadata| metadata.len()) {
            Ok(size) => size,
            Err(error) => {
                return Err(ConfigStoreError::new(
                    ConfigStoreErrorKind::Backup,
                    error.to_string(),
                ));
            }
        };
        if current_size > MAX_CONFIG_BYTES as u64 {
            return Err(ConfigStoreError::new(
                ConfigStoreErrorKind::TooLarge,
                format!("existing configuration exceeds {MAX_CONFIG_BYTES} bytes"),
            ));
        }
        let previous = read_bounded_bytes(path).map_err(|error| {
            let kind = if error.kind() == ConfigStoreErrorKind::TooLarge {
                ConfigStoreErrorKind::TooLarge
            } else {
                ConfigStoreErrorKind::Backup
            };
            ConfigStoreError::new(kind, error.detail().to_owned())
        })?;
        if let Err(error) = replace_file(backup, &previous) {
            return Err(ConfigStoreError::new(
                ConfigStoreErrorKind::Backup,
                error.to_string(),
            ));
        }
    }

    if let Err(error) = replace_file(path, bytes) {
        return Err(ConfigStoreError::new(
            ConfigStoreErrorKind::Rename,
            error.to_string(),
        ));
    }
    Ok(())
}

fn replace_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    AtomicFile::new(path, AllowOverwrite)
        .write(|file| {
            file.write_all(bytes)?;
            file.sync_all()
        })
        .map(|_| ())
        .map_err(io::Error::from)
}

#[cfg(test)]
#[path = "../tests/headless/application_config_store_tests.rs"]
mod tests;
