//! Pure string parsing for Intel engine sysfs directory names — no I/O.
//!
//! Shared by both drivers: `i915` exposes per-INSTANCE names (`rcs0`, `bcs1`,
//! `vcs0`, `vecs0`, `ccs0` — class mnemonic + digit instance), while `xe`
//! (Intel Core Ultra / Xe-LPG) exposes bare CLASS mnemonics with no instance
//! digit (`rcs`, `bcs`, `vcs`, `vecs`, `ccs`) or the long-form names
//! (`render`, `copy`, ...). Both drivers also accept the long-form names. The
//! class ids are the i915 UAPI `drm_i915_gem_engine_class`, reused verbatim by
//! xe. Ported minimal from `crates/taskmanager-platform-linux`'s `intel/pmu.rs`
//! (which is `pub(crate)` so unreachable here); kept in sync with its label
//! vocabulary so the helper's output lines up with the rest of TaskForest.

/// i915 UAPI engine class ids (`drm_i915_gem_engine_class`), shared by xe.
pub const CLASS_RENDER: u32 = 0;
pub const CLASS_COPY: u32 = 1;
pub const CLASS_VIDEO: u32 = 2;
pub const CLASS_VIDEO_ENHANCE: u32 = 3;
pub const CLASS_COMPUTE: u32 = 4;

/// The stable lowercase class keyword used as the JSON engine `class` field.
/// Unknown future classes pass through as `"unknown"` rather than a guess.
pub const fn class_keyword(class: u32) -> &'static str {
    match class {
        CLASS_RENDER => "render",
        CLASS_COPY => "copy",
        CLASS_VIDEO => "video",
        CLASS_VIDEO_ENHANCE => "video-enhance",
        CLASS_COMPUTE => "compute",
        _ => "unknown",
    }
}

/// Map an Intel engine sysfs directory name to the provider-neutral display
/// label shared with the rest of TaskForest. Tolerates both the `xe` per-class
/// names (`render`, `copy`, `compute`, `video`, `video-enhance`) and the legacy
/// `i915` per-instance names (`rcs0`, `bcs0`, `vcs0`, `vecs0`, `ccs0`). Unknown
/// future engines pass through upper-cased (separators → spaces) rather than
/// being dropped behind a list.
///
/// Order matters: the encode buckets (`vecs`, `video-enhance`) are matched
/// before the decode buckets (`vcs`, `video`) so an encode engine is never
/// swallowed by the decode label.
pub fn engine_label(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if lower.starts_with("vecs") || lower.starts_with("video-enhance") {
        "Video Encode".to_string()
    } else if lower.starts_with("vcs") || lower == "video" || lower.starts_with("video-decode") {
        "Video Decode".to_string()
    } else if lower.starts_with("rcs") || lower == "render" {
        "Render/3D".to_string()
    } else if lower.starts_with("bcs") || lower == "copy" || lower == "blitter" {
        "Copy".to_string()
    } else if lower.starts_with("ccs") || lower == "compute" {
        "Compute".to_string()
    } else {
        name.replace(['-', '_'], " ").to_ascii_uppercase()
    }
}

/// A parsed engine: its class id plus the instance digit (0 for xe bare/long
/// names, which are system-wide per class).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedEngine {
    pub class: u32,
    pub instance: u32,
}

/// Parse an `i915` engine sysfs directory name into `(class, instance)`.
///
/// Requires the i915 per-instance shape (`rcs0`, `bcs1`, …): class prefix + a
/// DIGIT instance suffix. The bare mnemonic (`rcs`) and long-form names collapse
/// to instance 0. Unknown names yield `None` (skipped, never fabricated).
pub fn parse_i915_engine(name: &str) -> Option<ParsedEngine> {
    let lower = name.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("vecs") {
        // vecs before vcs: encode must not collapse into decode.
        return build_parsed(CLASS_VIDEO_ENHANCE, rest);
    }
    if let Some(rest) = lower.strip_prefix("vcs") {
        return build_parsed(CLASS_VIDEO, rest);
    }
    if let Some(rest) = lower.strip_prefix("rcs") {
        return build_parsed(CLASS_RENDER, rest);
    }
    if let Some(rest) = lower.strip_prefix("bcs") {
        return build_parsed(CLASS_COPY, rest);
    }
    if let Some(rest) = lower.strip_prefix("ccs") {
        return build_parsed(CLASS_COMPUTE, rest);
    }
    // xe-style long-form names: collapsed to instance 0.
    let class = long_form_class(&lower)?;
    Some(ParsedEngine { class, instance: 0 })
}

/// Parse a `xe` engine sysfs directory name into `(class, instance=0)`.
///
/// The `xe` driver registers engines with the i915 per-instance vocabulary
/// MINUS the digit suffix — bare class mnemonics (`rcs`, `bcs`, `vcs`, `vecs`,
/// `ccs`) — and on some kernels the long-form class name (`render`, `copy`,
/// …). Mainline `xe_hw_engine_class_sysfs.c` exposes neither an instance digit
/// nor a `busy` node, and the per-class xe PMU counters are system-wide, so
/// instance is always 0. An optional all-digit tail (`rcs0`) is tolerated
/// defensively; a non-digit tail (`rcsX`) is rejected. Unknown names → `None`.
pub fn parse_xe_engine(name: &str) -> Option<ParsedEngine> {
    let lower = name.to_ascii_lowercase();
    let class = xe_mnemonic_class(&lower).or_else(|| long_form_class(&lower))?;
    Some(ParsedEngine { class, instance: 0 })
}

fn build_parsed(class: u32, instance_suffix: &str) -> Option<ParsedEngine> {
    let instance: u32 = instance_suffix.parse().ok()?;
    Some(ParsedEngine { class, instance })
}

/// Bare i915-style mnemonic with NO required instance digit — the layout `xe`
/// registers on Intel Core Ultra / Xe-LPG. `vecs` is matched before `vcs` so an
/// encode engine is never swallowed by decode. An optional all-digit tail
/// (`rcs0`) is tolerated; a non-digit tail (`rcsX`) is not.
fn xe_mnemonic_class(lower: &str) -> Option<u32> {
    let (prefix, class) = if lower.starts_with("vecs") {
        ("vecs", CLASS_VIDEO_ENHANCE)
    } else if lower.starts_with("vcs") {
        ("vcs", CLASS_VIDEO)
    } else if lower.starts_with("rcs") {
        ("rcs", CLASS_RENDER)
    } else if lower.starts_with("bcs") {
        ("bcs", CLASS_COPY)
    } else if lower.starts_with("ccs") {
        ("ccs", CLASS_COMPUTE)
    } else {
        return None;
    };
    let tail = &lower[prefix.len()..];
    let valid_tail = tail.is_empty() || tail.bytes().all(|byte| byte.is_ascii_digit());
    valid_tail.then_some(class)
}

/// Long-form xe/i915 class names (`render`, `copy`, …). Exact match only —
/// these never carry an instance suffix.
fn long_form_class(lower: &str) -> Option<u32> {
    Some(match lower {
        "render" => CLASS_RENDER,
        "copy" | "blitter" => CLASS_COPY,
        "compute" => CLASS_COMPUTE,
        "video" | "video-decode" => CLASS_VIDEO,
        "video-enhance" => CLASS_VIDEO_ENHANCE,
        _ => return None,
    })
}

#[cfg(test)]
#[path = "../tests/headless/privilege_engine_names.rs"]
mod tests;
