//! The Disk page's bounded mount-point slot.
//!
//! A mount path is the longest fact in a partition's identity row, and a
//! deep path (`/run/media/<user>/<volume>`) would push the row past its
//! panel on a narrow window. The path is therefore middle-ellipsized
//! inside its OWN bounded slot: the mount root and the volume name
//! survive, the middle is replaced by an ellipsis, and the identity row
//! keeps its bounds so the panel's other facts (the filesystem segment and
//! the capacity row) are never pushed out of the frame.

use taskmanager_application::i18n::t;
use taskmanager_core::core::metrics::DiskPartition;
use taskmanager_theme::tokens;

/// The bounded slot a partition's mount path may occupy inside its
/// identity row.
pub(crate) const MOUNT_SLOT_WIDTH: f32 = 150.0;

/// The partition identity row's text size (the shared FONT_12 body size).
pub(crate) fn partition_label_size() -> f32 {
    f32::from(tokens::FONT_12)
}

/// Middle-ellipsize one path so it fits `max_width`: the head (the mount
/// root) and the tail (the volume name) survive and the middle is replaced
/// by an ellipsis. A path that already fits is returned unchanged, and the
/// tail is never dropped while the head still has characters to give.
///
/// The width is the MEASURED shaped extent
/// ([`crate::text_metrics::measured_text_width`]), so proportional glyphs
/// and CJK are handled by the same metric the renderer paints with.
#[must_use]
pub(crate) fn middle_ellipsize(text: &str, max_width: f32) -> String {
    if text.is_empty() || !max_width.is_finite() || max_width <= 0.0 {
        return text.to_owned();
    }
    if crate::text_metrics::measured_text_width(text, partition_label_size()) <= max_width {
        return text.to_owned();
    }
    let characters: Vec<char> = text.chars().collect();
    if characters.len() < 3 {
        return text.to_owned();
    }
    let mut head = characters.len().div_ceil(2);
    let mut tail = characters.len() - head;
    let candidate = |head: usize, tail: usize| {
        let mut buffer: String = characters[..head].iter().collect();
        buffer.push('\u{2026}');
        buffer.extend(characters[characters.len() - tail..].iter());
        buffer
    };
    loop {
        let buffer = candidate(head, tail);
        if crate::text_metrics::measured_text_width(&buffer, partition_label_size()) <= max_width {
            return buffer;
        }
        if head <= 1 && tail <= 1 {
            break;
        }
        // Shrink the longer side first, so the head and the tail stay
        // balanced instead of one side vanishing.
        if tail >= head && tail > 1 {
            tail -= 1;
        } else if head > 1 {
            head -= 1;
        } else {
            tail -= 1;
        }
    }
    let fallback = candidate(1, characters.len().saturating_sub(1));
    if crate::text_metrics::measured_text_width(&fallback, partition_label_size()) <= max_width {
        fallback
    } else {
        String::new()
    }
}

/// One partition's identity: `name · mount · fs` with `/dev/` handling and
/// the unmounted qualifier (GPUI `partition_label` parity). The mount path
/// is middle-ellipsized inside its own bounded slot
/// ([`MOUNT_SLOT_WIDTH`]), so a long mount path never pushes the identity
/// row out of the panel.
pub(crate) fn partition_label(partition: &DiskPartition, mount_slot: f32) -> String {
    let raw_name = partition.name.trim();
    let raw_mount = partition.mount_point.trim();
    let is_windows_path = raw_name.contains(':')
        || raw_name.starts_with('\\')
        || raw_mount.contains(':')
        || raw_mount.starts_with('\\')
        || cfg!(target_os = "windows");
    let name = raw_name.trim_start_matches("/dev/");
    let mount = raw_mount.trim_start_matches("/dev/");
    let shown_mount = middle_ellipsize(mount, mount_slot);
    let prefix = if !is_windows_path && !name.starts_with('/') && !name.is_empty() {
        "/dev/"
    } else {
        ""
    };

    if mount.is_empty() {
        format!("{prefix}{name} · {}", t("disk.unmounted"))
    } else if partition.fs_type.is_empty() {
        if name.eq_ignore_ascii_case(mount) || name.is_empty() {
            shown_mount
        } else {
            format!("{prefix}{name} · {shown_mount}")
        }
    } else if name.eq_ignore_ascii_case(mount) || name.is_empty() {
        format!("{shown_mount} · {}", partition.fs_type)
    } else {
        format!("{prefix}{name} · {shown_mount} · {}", partition.fs_type)
    }
}
