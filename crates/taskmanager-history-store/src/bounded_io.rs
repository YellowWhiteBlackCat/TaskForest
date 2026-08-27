//! Bounded file reads shared by history queries, retention and boot evidence.

use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

use crate::{HistoryStoreError, HistoryStoreErrorKind};

/// Read at most `max_bytes`, detecting both an already-oversized file and a
/// file that grows after its metadata was sampled. Callers get a typed limit
/// failure instead of allocating according to untrusted on-disk length.
pub(crate) fn read_file(path: &Path, max_bytes: u64) -> Result<Vec<u8>, HistoryStoreError> {
    let file = File::open(path).map_err(|error| {
        HistoryStoreError::new(
            HistoryStoreErrorKind::Read,
            format!("{}: {error}", path.display()),
        )
    })?;
    let length = file.metadata().map_err(|error| {
        HistoryStoreError::new(
            HistoryStoreErrorKind::Read,
            format!("{}: {error}", path.display()),
        )
    })?;
    if length.len() > max_bytes {
        return Err(limit_error(path, length.len(), max_bytes));
    }

    let read_limit = max_bytes.saturating_add(1);
    let capacity = length.len().min(max_bytes).try_into().unwrap_or(usize::MAX);
    let mut bytes = Vec::with_capacity(capacity);
    file.take(read_limit)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            HistoryStoreError::new(
                HistoryStoreErrorKind::Read,
                format!("{}: {error}", path.display()),
            )
        })?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max_bytes {
        return Err(limit_error(
            path,
            u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            max_bytes,
        ));
    }
    Ok(bytes)
}

fn limit_error(path: &Path, actual: u64, maximum: u64) -> HistoryStoreError {
    HistoryStoreError::new(
        HistoryStoreErrorKind::ResourceLimit,
        format!(
            "{} is {actual} bytes; bounded read maximum is {maximum}",
            path.display()
        ),
    )
}

/// Stream one bounded file line by line without materializing its bytes.
///
/// `consume` sees every newline-delimited chunk (without the terminator) in
/// file order; a final chunk without a trailing newline is still delivered.
/// The byte ceiling is enforced against the bytes actually read — including
/// terminators — so memory stays bounded by the caller's per-line handling,
/// not by the file size. `\n` never appears inside a UTF-8 multi-byte
/// sequence, so splitting on it cannot cut a character in half.
pub(crate) fn for_each_line_bounded(
    path: &Path,
    max_bytes: u64,
    mut consume: impl FnMut(&str),
) -> Result<(), HistoryStoreError> {
    let file = File::open(path).map_err(|error| {
        HistoryStoreError::new(
            HistoryStoreErrorKind::Read,
            format!("{}: {error}", path.display()),
        )
    })?;
    let length = file.metadata().map_err(|error| {
        HistoryStoreError::new(
            HistoryStoreErrorKind::Read,
            format!("{}: {error}", path.display()),
        )
    })?;
    if length.len() > max_bytes {
        return Err(limit_error(path, length.len(), max_bytes));
    }
    let mut reader = BufReader::new(file).take(max_bytes.saturating_add(1));
    let mut consumed: u64 = 0;
    let mut buffer: Vec<u8> = Vec::new();
    loop {
        buffer.clear();
        let read = reader.read_until(b'\n', &mut buffer).map_err(|error| {
            HistoryStoreError::new(
                HistoryStoreErrorKind::Read,
                format!("{}: {error}", path.display()),
            )
        })?;
        if read == 0 {
            return Ok(());
        }
        consumed = consumed.saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
        if consumed > max_bytes {
            return Err(limit_error(path, consumed, max_bytes));
        }
        if buffer.last() == Some(&b'\n') {
            buffer.pop();
            if buffer.last() == Some(&b'\r') {
                buffer.pop();
            }
        }
        let line = String::from_utf8_lossy(&buffer);
        consume(&line);
    }
}
