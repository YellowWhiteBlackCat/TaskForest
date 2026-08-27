//! Background `iostat -d -K` sampler and its line parser, split out of
//! [`super`] (ADR-019). Only transfers-per-second can be honestly projected
//! onto `DiskScalarObservations::iops`; the combined read+write KiB/s that
//! `iostat` also emits is discarded because the scalar surface has separate
//! read/write slots and `iostat` cannot split them.

use std::collections::HashMap;
use std::io::BufRead;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// One disk's rate sample parsed from `iostat -d -K`. Only `iops` (transfers
/// per second) can be honestly projected onto `DiskScalarObservations`; the
/// combined read+write KiB/s that `iostat` also emits is intentionally
/// discarded because the scalar surface has separate
/// `read_bytes_per_sec`/`write_bytes_per_sec` slots and no combined-total
/// field, and `iostat` cannot split them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct DiskRates {
    /// Transfers per second (iostat `tps`).
    pub(crate) iops: u64,
}

/// Whether a whitespace-tokenized line is an iostat disk-name header — every
/// token matches `diskNNNN`. This excludes the `KB/t tps KB/s` column-header
/// line and any data row.
pub(crate) fn is_iostat_disk_header_line(tokens: &[&str]) -> bool {
    !tokens.is_empty()
        && tokens.iter().all(|token| {
            token.len() > 4
                && token.starts_with("disk")
                && token[4..].bytes().all(|b| b.is_ascii_digit())
        })
}

/// Parse one iostat data row into per-disk `(name, DiskRates)` pairs using the
/// most recently captured disk-name header. Returns `None` if the row's token
/// count is below `disk_names.len() * 3` or any token fails to parse as a
/// number. Column layout per disk: `KB/t tps KB/s`.
pub(crate) fn parse_iostat_data_line(
    tokens: &[&str],
    disk_names: &[String],
) -> Option<Vec<(String, DiskRates)>> {
    if disk_names.is_empty() || tokens.len() < disk_names.len() * 3 {
        return None;
    }
    let mut out = Vec::with_capacity(disk_names.len());
    for (idx, name) in disk_names.iter().enumerate() {
        let base = idx * 3;
        let _kb_per_transfer: f64 = tokens.get(base)?.parse().ok()?;
        let transfers_per_sec: f64 = tokens.get(base + 1)?.parse().ok()?;
        let _combined_kib_per_sec: f64 = tokens.get(base + 2)?.parse().ok()?;
        out.push((
            name.clone(),
            DiskRates {
                iops: transfers_per_sec.max(0.0) as u64,
            },
        ));
    }
    Some(out)
}

/// Parse a complete `iostat -d -K` excerpt (any number of header/data rows,
/// in any order) and return per-disk rates from the LAST successfully parsed
/// data row. Disk-name header lines update the known disk list; column-header
/// lines (`KB/t tps KB/s`) and any other non-numeric junk are skipped because
/// their tokens fail `f64` parsing inside [`parse_iostat_data_line`].
#[must_use]
pub(crate) fn parse_iostat_excerpt(text: &str) -> Option<HashMap<String, DiskRates>> {
    let mut disk_names: Vec<String> = Vec::new();
    let mut latest: Option<HashMap<String, DiskRates>> = None;
    for line in text.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }
        if is_iostat_disk_header_line(&tokens) {
            disk_names = tokens.iter().map(ToString::to_string).collect();
            continue;
        }
        if disk_names.is_empty() {
            continue;
        }
        if let Some(pairs) = parse_iostat_data_line(&tokens, &disk_names) {
            latest = Some(pairs.into_iter().collect());
        }
    }
    latest
}

/// Spawn the long-running `iostat -d -w 1 -K` child plus its reader thread.
/// Returns `None` (sampler disabled) when the binary cannot be spawned — the
/// caller then reports `MissingDependency` on every refresh. The reader thread
/// replaces the shared sample on every complete data row and exits cleanly
/// when the child's stdout closes (EOF on `BufRead::lines`).
pub(crate) fn spawn_iostat_sampler(
    store: Arc<Mutex<Option<HashMap<String, DiskRates>>>>,
) -> Option<(std::process::Child, JoinHandle<()>)> {
    let mut command = std::process::Command::new("iostat");
    command
        .args(["-d", "-w", "1", "-K"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command.spawn().ok()?;
    let stdout = child.stdout.take()?;
    let handle = match std::thread::Builder::new()
        .name("taskforest-macos-iostat".to_owned())
        .spawn(move || {
            let reader = std::io::BufReader::new(stdout);
            let mut buf = String::new();
            for line in reader.lines() {
                let Ok(line) = line else { break };
                buf.push_str(&line);
                buf.push('\n');
                // Bound the buffer to the tail (~4 KiB, trimmed at line
                // boundaries) so a long-lived reader never grows without limit.
                while buf.len() > 4096 {
                    match buf.find('\n') {
                        Some(idx) => {
                            buf.drain(..=idx);
                        }
                        None => buf.clear(),
                    }
                }
                let tokens: Vec<&str> = line.split_whitespace().collect();
                if tokens.is_empty() || is_iostat_disk_header_line(&tokens) {
                    continue;
                }
                // Only re-parse when the new line is plausibly a data row (every
                // token parses as a number); this skips the `KB/t tps KB/s`
                // column-header line and any other non-numeric noise without
                // consulting the (possibly not-yet-seen) disk-name header.
                if !tokens.iter().all(|token| token.parse::<f64>().is_ok()) {
                    continue;
                }
                if let Some(map) = parse_iostat_excerpt(&buf)
                    && let Ok(mut guard) = store.lock()
                {
                    *guard = Some(map);
                }
            }
        }) {
        Ok(handle) => handle,
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
    };
    Some((child, handle))
}
