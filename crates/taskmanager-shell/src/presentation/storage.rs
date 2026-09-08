//! Storage presentation labels shared by all frontends.

use taskmanager_application::i18n;
use taskmanager_core::core::storage_health::FilesystemBackingKind;

/// Stable localized label for filesystem topology class. The label is only a
/// presentation spelling; the backing classification itself remains core
/// truth and is never inferred by a frontend.
#[must_use]
pub fn filesystem_backing_label(kind: FilesystemBackingKind) -> &'static str {
    match kind {
        FilesystemBackingKind::PhysicalBlock => i18n::t("disk.backing_physical"),
        FilesystemBackingKind::BtrfsSubvolume => i18n::t("disk.backing_btrfs"),
        FilesystemBackingKind::ZfsDataset => i18n::t("disk.backing_zfs"),
        FilesystemBackingKind::Overlay => i18n::t("disk.backing_overlay"),
        FilesystemBackingKind::Tmpfs => i18n::t("disk.backing_tmpfs"),
        FilesystemBackingKind::Network => i18n::t("disk.backing_network"),
        FilesystemBackingKind::Unknown => i18n::t("disk.backing_unknown"),
    }
}
