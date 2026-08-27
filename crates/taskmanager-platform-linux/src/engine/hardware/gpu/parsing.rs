//! Pure GPU sysfs value and driver-name parsers.

/// Parse an AMD amdgpu `*_busy_percent` sysfs value (an integer 0–100) into a
/// clamped `f32`. Returns `None` on empty/unparseable input. Negatives and
/// values over 100 (which amdgpu shouldn't emit but defensively...) are clamped
/// to the valid `[0.0, 100.0]` range.
///
/// Pulled out as a pure function so it can be unit-tested with mock sysfs
/// strings without a real GPU.
pub(super) fn parse_busy_percent(raw: &str) -> Option<f32> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    t.parse::<f32>().ok().map(|v| v.clamp(0.0, 100.0))
}

/// Pure helper: extract the driver name from a `device/driver` symlink target.
/// The basename of the kernel driver path is the driver itself, e.g.
/// `"../../../../bus/pci/drivers/xe"` → `Some("xe")`,
/// `"/sys/bus/pci/drivers/amdgpu"` → `Some("amdgpu")`. Returns `None` for an
/// empty/garbage target. Tolerates surrounding whitespace AND a trailing slash
/// (readlink of a dir symlink can yield `".../i915/"`). Pure (string-only) so
/// it can be unit-tested without a real sysfs symlink.
pub(super) fn parse_driver_name(symlink_target: &str) -> Option<String> {
    let t = symlink_target.trim().trim_end_matches('/');
    if t.is_empty() {
        return None;
    }
    let basename = t.rsplit('/').next()?;
    if basename.is_empty() {
        None
    } else {
        Some(basename.to_string())
    }
}

/// Pure helper: compose the best dep-free GPU brand string for an Intel PCI
/// display device from its driver name. `xe` (newer Intel integrated graphics,
/// Arrow Lake / Xe LPG) → "Intel Xe Graphics"; the legacy `i915` driver (Gen11
/// and earlier integrated) and any other / missing Intel driver →
/// "Intel Graphics". Dep-free by design: there is no model-name sysfs node on
/// Intel iGPUs, and the task scope forbids a PCI-DB dependency.
pub(super) fn compose_intel_brand(driver: Option<&str>) -> &'static str {
    match driver {
        Some("xe") => "Intel Xe Graphics",
        Some(_) | None => "Intel Graphics",
    }
}

/// Parse one PCI ID from sysfs or a pci.ids token without accepting overflow
/// or trailing syntax.
pub(super) fn parse_pci_id(raw: &str) -> Option<u16> {
    let value = raw.trim().strip_prefix("0x").unwrap_or(raw.trim());
    (value.len() == 4)
        .then(|| u16::from_str_radix(value, 16).ok())
        .flatten()
}

/// Resolve a device name from the bounded, read-only `pci.ids` text format.
/// Device names are only accepted under the requested vendor; subsystem lines
/// are intentionally ignored.
pub(super) fn pci_ids_device_name(text: &str, vendor: u16, device: u16) -> Option<String> {
    let mut vendor_match = false;
    for line in text.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if !line.starts_with(['\t', ' ']) {
            vendor_match = line.split_whitespace().next().and_then(parse_pci_id) == Some(vendor);
            continue;
        }
        if !vendor_match || !line.starts_with('\t') || line.starts_with("\t\t") {
            continue;
        }
        let mut fields = line.trim().splitn(2, char::is_whitespace);
        let Some(raw_device) = fields.next() else {
            continue;
        };
        if parse_pci_id(raw_device) != Some(device) {
            continue;
        }
        return fields
            .next()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned);
    }
    None
}

/// Prefer the bracketed marketing name used by modern pci.ids entries, while
/// preserving the complete device label for entries without one.
pub(super) fn marketing_name_from_pci_label(label: &str) -> Option<String> {
    let label = label.trim();
    if label.is_empty() {
        return None;
    }
    if let Some(start) = label.rfind('[')
        && let Some(end) = label[start + 1..].find(']')
    {
        let marketing = label[start + 1..start + 1 + end].trim();
        if !marketing.is_empty() {
            return Some(marketing.to_owned());
        }
    }
    Some(label.to_owned())
}

/// Normalize a PCI address to Linux `dddd:bb:dd.f` notation.
///
/// This is provider-neutral identity parsing. NVML is one caller, but the
/// canonical address belongs to the runtime device merge rather than to a
/// vendor backend and therefore remains available in reduced test builds.
#[cfg(any(test, feature = "nvidia"))]
pub(crate) fn normalize_pci_slot(raw: &str) -> Option<String> {
    let mut parts = raw.trim().split(':');
    let domain = u32::from_str_radix(parts.next()?, 16).ok()?;
    let bus = u32::from_str_radix(parts.next()?, 16).ok()?;
    let (device, function) = parts.next()?.split_once('.')?;
    if parts.next().is_some() {
        return None;
    }
    let device = u32::from_str_radix(device, 16).ok()?;
    let function = u32::from_str_radix(function, 16).ok()?;
    (domain <= 0xffff && bus <= 0xff && device <= 0x1f && function <= 7)
        .then(|| format!("{domain:04x}:{bus:02x}:{device:02x}.{function:x}"))
}
