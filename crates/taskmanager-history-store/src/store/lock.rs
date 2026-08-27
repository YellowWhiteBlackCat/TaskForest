//! Fail-closed single-writer ownership of one history directory.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::HistoryStoreError;
use crate::bounded_io;

const LOCK_FILE: &str = "history.lock";
const MAX_LOCK_CLAIM_BYTES: u64 = 128;
static OWNER_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Read-only observation of the single-writer claim. `Ambiguous` is
/// deliberately fail-closed: malformed, unreadable or racing claims are never
/// promoted to a running collector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryWriterClaimStatus {
    Absent,
    Live { pid: u32 },
    Stale { pid: u32 },
    Ambiguous,
}

/// The open, exclusively locked file is the actual ownership primitive. The
/// token protects path cleanup if an external actor replaces the directory
/// entry while this owner is alive.
pub(super) struct RootLockOwnership {
    path: PathBuf,
    token: String,
    _file: Option<File>,
}

impl Drop for RootLockOwnership {
    fn drop(&mut self) {
        // Close the handle before unlinking: Windows refuses to delete a
        // file that is still open, which would strand this owner's claim on
        // disk and fail every later generation closed.
        self._file = None;
        let Ok(current) = bounded_io::read_file(&self.path, MAX_LOCK_CLAIM_BYTES) else {
            return;
        };
        if current == self.token.as_bytes() {
            let _ = fs::remove_file(&self.path);
        }
    }
}

pub(super) fn acquire_root_lock(
    root: &Path,
    holder_is_gone: fn(u32) -> bool,
) -> Result<RootLockOwnership, HistoryStoreError> {
    let path = root.join(LOCK_FILE);
    let token = owner_token();
    match OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(mut file) => {
            if let Err(error) = file.try_lock() {
                let _ = fs::remove_file(&path);
                return Err(locked_io_error(
                    &path,
                    "new claim could not be locked",
                    error.into(),
                ));
            }
            if let Err(error) = write_claim(&mut file, &token) {
                // This call created the path, so cleanup cannot erase a prior
                // owner. Preserve the original write error.
                let _ = fs::remove_file(&path);
                return Err(HistoryStoreError::new(
                    crate::HistoryStoreErrorKind::Write,
                    format!("{}: {error}", path.display()),
                ));
            }
            Ok(RootLockOwnership {
                path,
                token,
                _file: Some(file),
            })
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            acquire_existing_claim(path, token, holder_is_gone)
        }
        Err(error) => Err(HistoryStoreError::new(
            crate::HistoryStoreErrorKind::Open,
            format!("{}: {error}", path.display()),
        )),
    }
}

/// Probe writer liveness without acquiring or replacing the claim.
#[must_use]
pub fn probe_root_lock(root: &Path, holder_is_gone: fn(u32) -> bool) -> HistoryWriterClaimStatus {
    let path = root.join(LOCK_FILE);
    let mut file = match OpenOptions::new().read(true).open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return HistoryWriterClaimStatus::Absent;
        }
        Err(_) => return HistoryWriterClaimStatus::Ambiguous,
    };
    let Ok(claim) = read_claim_fail_closed(&mut file, &path) else {
        return HistoryWriterClaimStatus::Ambiguous;
    };
    if holder_is_gone(claim.pid) {
        HistoryWriterClaimStatus::Stale { pid: claim.pid }
    } else {
        HistoryWriterClaimStatus::Live { pid: claim.pid }
    }
}

fn acquire_existing_claim(
    path: PathBuf,
    token: String,
    holder_is_gone: fn(u32) -> bool,
) -> Result<RootLockOwnership, HistoryStoreError> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|error| locked_io_error(&path, "existing owner claim cannot be opened", error))?;
    file.try_lock()
        .map_err(|error| locked_io_error(&path, "another owner holds the claim", error.into()))?;
    let observed = read_claim_fail_closed(&mut file, &path)?;
    if !holder_is_gone(observed.pid) {
        return Err(locked_error(&path, "another live instance owns"));
    }
    write_claim(&mut file, &token).map_err(|error| {
        // This is an existing claim, so a failed takeover remains in place
        // and ambiguous rather than being deleted as though we created it.
        HistoryStoreError::new(
            crate::HistoryStoreErrorKind::Write,
            format!("{}: {error}", path.display()),
        )
    })?;
    Ok(RootLockOwnership {
        path,
        token,
        _file: Some(file),
    })
}

struct ObservedClaim {
    pid: u32,
}

/// An unreadable, empty or malformed claim is ambiguous, never proof of a
/// dead owner. The file is bounded before allocation.
fn read_claim_fail_closed(
    file: &mut File,
    path: &Path,
) -> Result<ObservedClaim, HistoryStoreError> {
    let length = file
        .metadata()
        .map_err(|error| locked_io_error(path, "owner claim metadata is unreadable", error))?
        .len();
    if length > MAX_LOCK_CLAIM_BYTES {
        return Err(locked_error(path, "existing owner claim exceeds its bound"));
    }
    file.rewind()
        .map_err(|error| locked_io_error(path, "owner claim cannot be rewound", error))?;
    let mut bytes = Vec::with_capacity(usize::try_from(length).unwrap_or(usize::MAX));
    file.take(MAX_LOCK_CLAIM_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| locked_io_error(path, "owner claim cannot be read", error))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_LOCK_CLAIM_BYTES {
        return Err(locked_error(
            path,
            "existing owner claim grew beyond its bound",
        ));
    }
    let raw = String::from_utf8(bytes)
        .map_err(|_| locked_error(path, "existing owner claim is not UTF-8"))?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(locked_error(path, "existing owner claim is incomplete"));
    }
    let pid_text = match trimmed.split_once(':') {
        Some((pid, sequence)) if !sequence.is_empty() && sequence.parse::<u64>().is_ok() => pid,
        Some(_) => return Err(locked_error(path, "existing owner token is malformed")),
        None => trimmed,
    };
    let pid = pid_text
        .parse::<u32>()
        .map_err(|_| locked_error(path, "existing owner claim is malformed"))?;
    Ok(ObservedClaim { pid })
}

fn write_claim(file: &mut File, token: &str) -> std::io::Result<()> {
    file.set_len(0)?;
    file.rewind()?;
    file.write_all(token.as_bytes())?;
    file.flush()
}

fn owner_token() -> String {
    format!(
        "{}:{}",
        std::process::id(),
        OWNER_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

fn locked_error(path: &Path, reason: &str) -> HistoryStoreError {
    HistoryStoreError::new(
        crate::HistoryStoreErrorKind::Locked,
        format!("{reason} {}", path.display()),
    )
}

fn locked_io_error(path: &Path, reason: &str, error: std::io::Error) -> HistoryStoreError {
    HistoryStoreError::new(
        crate::HistoryStoreErrorKind::Locked,
        format!("{reason} {}: {error}", path.display()),
    )
}
