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

/// Parse the version declared by a kernel module's `/sys/module/<name>/version`
/// attribute. The value is reported verbatim (one line, trimmed, e.g.
/// `550.107.02`); an empty or whitespace-only node is an honest absence, never
/// a fabricated version. Pure (string-only) so fixture tests never touch the
/// host's module tree.
pub(super) fn parse_module_version(raw: &str) -> Option<String> {
    let line = raw.lines().next()?.trim();
    (!line.is_empty()).then(|| line.to_owned())
}

/// Parse the kernel driver release from `/proc/driver/nvidia/version`. The
/// `NVRM version:` line embeds the release inside prose such as
/// `NVRM version: NVIDIA UNIX x86_64 Kernel Module  550.107.02  Tue Oct ...`,
/// so the first `major.minor[.patch]` token is the version. Lines like
/// `GCC version:` or `Loaded:` are ignored; a file without a parseable NVRM
/// token stays absent.
pub(super) fn parse_nvrm_driver_version(raw: &str) -> Option<String> {
    raw.lines()
        .find_map(|line| line.strip_prefix("NVRM version:"))
        .and_then(|prose| {
            prose.split_whitespace().find_map(|token| {
                let mut components = token.split('.');
                let major = components.next()?.parse::<u32>().ok()?;
                let minor = components.next()?.parse::<u32>().ok()?;
                if components
                    .next()
                    .is_some_and(|part| part.parse::<u32>().is_err())
                {
                    return None;
                }
                (major > 0 || minor > 0).then_some(token)
            })
        })
        .map(ToOwned::to_owned)
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
