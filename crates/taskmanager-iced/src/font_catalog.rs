//! Safe system-font discovery for the Iced settings surface.
//!
//! Iced does not expose its renderer's internal `fontdb` through the public
//! application API. The renderer already uses the mature `fontdb` crate, so
//! this frontend uses the same safe database API to enumerate installed
//! families. The neutral theme layer owns deduplication, validation, bounded
//! interning, and product-face separation; this module only adapts the crate's
//! face records to that neutral input.

use std::sync::OnceLock;

use taskmanager_theme::{FONT_MISANS_VF, FONT_ROBOTO_MONO, FontAvailability};

static SYSTEM_CATALOG: OnceLock<FontAvailability> = OnceLock::new();

/// Return a cached snapshot of the families reported by the host font files.
/// The scan occurs once per process, before the first real Iced window opens;
/// later settings renders are pure reads and cannot repeat filesystem work.
pub(crate) fn system() -> FontAvailability {
    SYSTEM_CATALOG
        .get_or_init(|| {
            let mut database = fontdb::Database::new();
            database.load_system_fonts();
            let families = database
                .faces()
                .flat_map(|face| face.families.iter().map(|(family, _)| family.as_str()));
            FontAvailability::from_installed_families(
                families.chain([FONT_MISANS_VF, FONT_ROBOTO_MONO]),
            )
        })
        .clone()
}

/// Deterministic no-I/O catalog for demo fixtures and headless tests. The
/// bundled names are included because the launcher registers those bytes
/// before the first Iced frame is rendered.
pub(crate) fn bundled_only() -> FontAvailability {
    FontAvailability::from_installed_families([FONT_MISANS_VF, FONT_ROBOTO_MONO])
}

#[cfg(test)]
#[path = "../tests/gui/font_catalog_tests.rs"]
mod tests;
