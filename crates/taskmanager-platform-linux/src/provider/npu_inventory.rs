//! Linux NPU accelerator inventory provider (capability `accelerator.npu`).
//!
//! Discovery-first: one call enumerates the DRM accelerator
//! class (`/sys/class/accel`, Linux 6.3+) and reports each node's kernel
//! identity plus its bound driver name. An absent class directory is the
//! honest "no NPU subsystem" success — an empty device list, never a failure.
//! Utilization and memory facts stay typed `Unavailable(Unsupported)` until a
//! stable kernel interface exists (the ivpu fdinfo surface is still moving);
//! no curve or capacity is ever fabricated here.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use taskmanager_core::{
    DeviceId, FailureKind, NpuDevice, NpuInventorySnapshot, NpuMemoryReport, ScalarObservation,
};
use taskmanager_platform_contract::ProviderFailure;
use taskmanager_platform_provider::NpuInventoryProvider;

/// Bounded enumeration: a host with more accelerator nodes than this is not a
/// shape this product renders usefully, and the read stays bounded.
const MAX_ACCEL_NODES: usize = 16;

pub(super) struct NativeNpuInventoryProvider {
    accel_root: PathBuf,
}

impl NativeNpuInventoryProvider {
    pub(super) fn new() -> Self {
        Self {
            accel_root: PathBuf::from("/sys/class/accel"),
        }
    }
}

impl Default for NativeNpuInventoryProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl NpuInventoryProvider for NativeNpuInventoryProvider {
    fn read_inventory(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<NpuInventorySnapshot, ProviderFailure> {
        discover_accelerators(&self.accel_root, observed_at_ms)
    }
}

/// Enumerate one sysfs accelerator class root. A missing root is an honest
/// empty host; unreadable roots stay typed failures; each accepted node uses
/// its canonical physical sysfs path rather than the enumerated `accelN` name.
fn discover_accelerators(
    root: &Path,
    observed_at_ms: u64,
) -> Result<NpuInventorySnapshot, ProviderFailure> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(NpuInventorySnapshot::discovered(Vec::new(), observed_at_ms));
        }
        Err(error) => return Err(io_failure(error)),
    };
    let mut devices = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => return Err(io_failure(error)),
        };
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !is_accel_node_name(name) {
            continue;
        }
        devices.push(accel_device(entry.path(), name));
        if devices.len() >= MAX_ACCEL_NODES {
            break;
        }
    }
    Ok(NpuInventorySnapshot::discovered(devices, observed_at_ms))
}

/// `accel<digits>` exactly: unrelated class members are skipped, not errors.
fn is_accel_node_name(name: &str) -> bool {
    match name.strip_prefix("accel") {
        Some(suffix) => !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit()),
        None => false,
    }
}

/// One discovered node. The bound driver name is an optional fact: an unbound
/// node (no `device/driver` symlink) keeps `driver: None` rather than failing
/// the whole enumeration.
fn accel_device(node_root: PathBuf, _name: &str) -> NpuDevice {
    let driver = fs::read_link(node_root.join("device").join("driver"))
        .ok()
        .and_then(|target| {
            target
                .file_name()
                .map(|segment| segment.to_string_lossy().into_owned())
        })
        .filter(|driver| !driver.is_empty());
    NpuDevice {
        device_id: DeviceId::new(
            fs::canonicalize(node_root.join("device"))
                .or_else(|_| fs::canonicalize(&node_root))
                .map_or_else(
                    |_| String::new(),
                    |path| format!("linux:npu:sysfs:{}", path.to_string_lossy()),
                ),
        ),
        brand: None,
        driver,
        utilization_pct: ScalarObservation::unavailable(FailureKind::Unsupported),
        engines: Vec::new(),
        memory: NpuMemoryReport {
            dedicated_total_bytes: ScalarObservation::unavailable(FailureKind::Unsupported),
            shared_total_bytes: ScalarObservation::unavailable(FailureKind::Unsupported),
            sram_total_bytes: read_sram_total(&node_root).map_or_else(
                || ScalarObservation::unavailable(FailureKind::Unsupported),
                |bytes| ScalarObservation::available(bytes, 0),
            ),
        },
        ..NpuDevice::default()
    }
}

/// Read an explicitly named SRAM resource when a kernel accelerator driver
/// exposes one. Drivers have used both a byte-valued sysfs node and a small
/// `mem_info` key/value file; accept only those names and units so an
/// unrelated memory counter cannot be relabelled as SRAM.
fn read_sram_total(node_root: &Path) -> Option<u64> {
    for path in [
        node_root.join("device/sram_size"),
        node_root.join("device/sram_total_bytes"),
        node_root.join("device/memory/sram_size"),
    ] {
        if let Ok(text) = fs::read_to_string(path)
            && let Some(bytes) = parse_size_bytes(text.trim())
        {
            return Some(bytes);
        }
    }
    let text = fs::read_to_string(node_root.join("device/mem_info")).ok()?;
    text.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        let key = key.trim().to_ascii_lowercase();
        key.contains("sram")
            .then(|| parse_size_bytes(value.trim()))
            .flatten()
    })
}

fn parse_size_bytes(value: &str) -> Option<u64> {
    let mut fields = value.split_whitespace();
    let number = fields.next()?.parse::<u64>().ok()?;
    let unit = fields.next().unwrap_or("bytes").to_ascii_lowercase();
    let multiplier = match unit.as_str() {
        "b" | "byte" | "bytes" => 1,
        "kib" | "kb" => 1024,
        "mib" | "mb" => 1024 * 1024,
        "gib" | "gb" => 1024 * 1024 * 1024,
        _ => return None,
    };
    number.checked_mul(multiplier)
}

fn io_failure(error: io::Error) -> ProviderFailure {
    let kind = match error.kind() {
        io::ErrorKind::PermissionDenied => FailureKind::PermissionDenied,
        _ => FailureKind::TemporarilyUnavailable,
    };
    ProviderFailure::from_kind(kind)
}

#[cfg(test)]
#[path = "../../tests/headless/linux_provider_npu_inventory_tests.rs"]
mod tests;
