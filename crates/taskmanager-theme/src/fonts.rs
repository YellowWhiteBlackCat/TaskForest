//! Font selection & resolution. The UI distinguishes two roles (UI +
//! monospace), each with a user preference ([`FontChoice`]) and a per-skin
//! system default. [`FontAvailability`] captures at startup which system
//! families actually exist on the host.
//!
//! Policy (2026-08): the bundled faces are PRIMARY — every role defaults to
//! [`FontChoice::Bundled`]. The UI role resolves to MiSans VF and the
//! monospace role resolves to Roboto Mono. This is deliberate: MiSans is
//! the product reading face (including CJK coverage), while Roboto Mono
//! is reserved for aligned metrics and diagnostic values. The per-skin system
//! families are consulted ONLY when the user explicitly opts into
//! [`FontChoice::System`] or [`FontChoice::Custom`], so hosts without the
//! selected family still render through a typed fallback instead of silently
//! requesting a missing face.

use std::sync::{Mutex, OnceLock};

use crate::theme::Skin;

/// Family names of the bundled faces registered via
/// `taskmanager_assets::embedded_fonts()`.
pub const FONT_MISANS_VF: &str = "MiSans VF";
pub const FONT_ROBOTO_MONO: &str = "Roboto Mono";

/// The two roles a user can pick a face for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FontRole {
    Ui,
    Mono,
}

/// User preference per role. `System` means the current skin's recommended
/// system family; `Custom` is a family observed in the bounded startup font
/// catalog. The derived default is [`FontChoice::Bundled`].
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FontChoice {
    System,
    /// A system family copied from [`FontAvailability::families`]. This is
    /// only constructed safely through [`FontAvailability::choice_for`].
    Custom(&'static str),
    #[default]
    Bundled,
}

/// User font intent, one choice per role. Default = bundled product faces
/// (MiSans VF for UI, Roboto Mono for mono).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FontPreference {
    pub ui: FontChoice,
    pub mono: FontChoice,
}

impl Default for FontPreference {
    fn default() -> Self {
        Self {
            ui: FontChoice::Bundled,
            mono: FontChoice::Bundled,
        }
    }
}

/// Concrete family names actually used by a theme (the input to
/// `Theme::build`). Every field is a family name gpui can resolve.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ResolvedFonts {
    pub ui: &'static str,
    pub mono: &'static str,
}

impl ResolvedFonts {
    /// The skin's own system families — pre-resolution behavior, used by
    /// tests and as the cold-start default before startup detects
    /// availability.
    pub const fn system_for(skin: Skin) -> Self {
        Self {
            ui: skin.ui_font(),
            mono: skin.mono_font(),
        }
    }
}

/// Per-skin snapshot of which system families exist on this host. Detected
/// once at startup (after font registration); skin switches re-resolve against
/// this map. The embedded flags and bounded family catalog are part of the
/// snapshot so a registration failure cannot be mistaken for a successful
/// bundled-font policy.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FontAvailability {
    per_skin: [ResolvedFonts; 4],
    bundled_ui: bool,
    bundled_mono: bool,
    families: Vec<&'static str>,
    custom_families: Vec<&'static str>,
    catalog_truncated: bool,
}

impl FontAvailability {
    /// Build the per-skin availability from the host's installed family
    /// names (case-insensitive). The toolkit query itself lives behind the
    /// theme's optional `gpui` feature (`gpui::detect_font_availability`),
    /// which hands this constructor the text system's family list.
    pub fn from_installed_families<'a>(installed: impl IntoIterator<Item = &'a str>) -> Self {
        let mut names = Vec::new();
        let mut catalog_truncated = false;
        for family in installed {
            let family = family.trim();
            if family.is_empty() {
                continue;
            }
            if names
                .iter()
                .any(|name: &&'static str| name.eq_ignore_ascii_case(family))
            {
                continue;
            }
            let Some(canonical) = intern_font_family(family) else {
                catalog_truncated = true;
                continue;
            };
            names.push(canonical);
        }
        names.sort_unstable_by_key(|name| name.to_ascii_lowercase());
        let has = |family: &'static str| names.iter().any(|name| name.eq_ignore_ascii_case(family));
        let bundled_ui = has(FONT_MISANS_VF);
        let bundled_mono = has(FONT_ROBOTO_MONO);
        let ui_fallback = if bundled_ui {
            FONT_MISANS_VF
        } else {
            first_installed(&names, UI_FALLBACK_FAMILIES, "Noto Sans")
        };
        let mono_fallback = if bundled_mono {
            FONT_ROBOTO_MONO
        } else {
            first_installed(&names, MONO_FALLBACK_FAMILIES, "Noto Sans Mono")
        };
        let custom_families = names
            .iter()
            .copied()
            .filter(|family| {
                !family.eq_ignore_ascii_case(FONT_MISANS_VF)
                    && !family.eq_ignore_ascii_case(FONT_ROBOTO_MONO)
            })
            .collect();
        let per_skin = Skin::ALL.map(|skin| ResolvedFonts {
            ui: if has(skin.ui_font()) {
                skin.ui_font()
            } else {
                ui_fallback
            },
            mono: if has(skin.mono_font()) {
                skin.mono_font()
            } else {
                mono_fallback
            },
        });
        Self {
            per_skin,
            bundled_ui,
            bundled_mono,
            families: names,
            custom_families,
            catalog_truncated,
        }
    }

    /// The system-or-bundled families for one skin (no user override applied).
    pub fn resolved_for(&self, skin: Skin) -> ResolvedFonts {
        self.per_skin[skin.idx()]
    }

    /// Whether the embedded product faces were observed in the toolkit font
    /// database after registration. This is a verified observation, not an
    /// assumption based on the embedded byte assets existing in the binary.
    pub const fn embedded_fonts_ready(&self) -> bool {
        self.bundled_ui && self.bundled_mono
    }

    /// Whether the UI face was registered successfully.
    pub const fn bundled_ui_available(&self) -> bool {
        self.bundled_ui
    }

    /// Whether the monospace face was registered successfully.
    pub const fn bundled_mono_available(&self) -> bool {
        self.bundled_mono
    }

    /// All observed font families, sorted case-insensitively. The list is
    /// bounded before it reaches the UI so a malformed or unusually large
    /// host catalog cannot create unbounded settings elements.
    pub fn families(&self) -> &[&'static str] {
        &self.families
    }

    /// Installed families that may be selected as a custom primary face.
    /// Bundled product faces remain separate `Bundled` choices so a user does
    /// not accidentally turn a product face into a skin-specific override.
    pub fn custom_families(&self) -> &[&'static str] {
        &self.custom_families
    }

    /// Resolve a persisted/user-supplied family name to its canonical catalog
    /// spelling. Unknown names stay unavailable and cannot become a primary
    /// font merely because they appeared in a config file.
    pub fn choice_for(&self, family: &str) -> Option<FontChoice> {
        let family = family.trim();
        if family.eq_ignore_ascii_case(FONT_MISANS_VF)
            || family.eq_ignore_ascii_case(FONT_ROBOTO_MONO)
        {
            return None;
        }
        self.custom_families
            .iter()
            .find(|name| name.eq_ignore_ascii_case(family.trim()))
            .copied()
            .map(FontChoice::Custom)
    }

    /// Whether the host catalog exceeded the fixed safety bound.
    pub const fn catalog_truncated(&self) -> bool {
        self.catalog_truncated
    }
}

const MAX_FONT_FAMILIES: usize = 2048;
const MAX_FONT_FAMILY_BYTES: usize = 256;

/// The toolkit returns borrowed family names, while `Theme` intentionally
/// stays `Copy` for the render projection. We therefore intern only the
/// bounded startup catalog into a process-lifetime read-only arena. The cap is
/// explicit (2048 names × 256 bytes) and prevents repeated settings changes or
/// hostile font metadata from causing unbounded memory growth. No raw pointer
/// crosses this API and no caller can mutate an interned name.
static FONT_INTERNER: OnceLock<Mutex<Vec<&'static str>>> = OnceLock::new();

/// Intern one validated family name for the startup catalog/config bridge.
/// Returns `None` when the fixed catalog budget is exhausted or the name is
/// not a valid bounded family label.
pub fn intern_font_family(family: &str) -> Option<&'static str> {
    let family = family.trim();
    if family.is_empty() || family.len() > MAX_FONT_FAMILY_BYTES {
        return None;
    }
    let arena = FONT_INTERNER.get_or_init(|| Mutex::new(Vec::new()));
    let mut arena = arena.lock().ok()?;
    if let Some(existing) = arena
        .iter()
        .find(|existing| existing.eq_ignore_ascii_case(family))
        .copied()
    {
        return Some(existing);
    }
    if arena.len() >= MAX_FONT_FAMILIES {
        return None;
    }
    let owned = family.to_owned().into_boxed_str();
    let interned: &'static str = Box::leak(owned);
    arena.push(interned);
    Some(interned)
}

const UI_FALLBACK_FAMILIES: [&str; 6] = [
    "Noto Sans",
    "Segoe UI Variable",
    "Segoe UI",
    "Arial",
    "Helvetica",
    "DejaVu Sans",
];

const MONO_FALLBACK_FAMILIES: [&str; 6] = [
    "Noto Sans Mono",
    "Cascadia Code",
    "Consolas",
    "Menlo",
    "DejaVu Sans Mono",
    "Liberation Mono",
];

fn first_installed(
    names: &[&'static str],
    candidates: [&'static str; 6],
    empty_fallback: &'static str,
) -> &'static str {
    if let Some(candidate) = candidates.into_iter().find(|candidate| {
        names
            .iter()
            .any(|name| name.eq_ignore_ascii_case(candidate))
    }) {
        return candidate;
    }
    // A non-empty catalog always yields an observed family, even when none of
    // the preferred platform candidates exist. The final literal is reached
    // only when the toolkit reports no families at all; that state is already
    // represented by the startup availability warning.
    names.first().copied().unwrap_or(empty_fallback)
}

/// Combine the user's preference with host availability into concrete family
/// names for `Theme::build`. Bundled resolves UI to MiSans VF and mono to
/// Roboto Mono when registration was verified. If registration failed,
/// the corresponding role uses the best installed, deterministic fallback
/// captured by [`FontAvailability`] instead of claiming a missing face.
pub fn resolve_fonts(pref: FontPreference, skin: Skin, avail: &FontAvailability) -> ResolvedFonts {
    let mut fonts = avail.resolved_for(skin);
    match pref.ui {
        FontChoice::Bundled if avail.bundled_ui_available() => fonts.ui = FONT_MISANS_VF,
        FontChoice::Custom(family) => {
            if let Some(FontChoice::Custom(canonical)) = avail.choice_for(family) {
                fonts.ui = canonical;
            }
        }
        FontChoice::System | FontChoice::Bundled => {}
    }
    match pref.mono {
        FontChoice::Bundled if avail.bundled_mono_available() => {
            fonts.mono = FONT_ROBOTO_MONO;
        }
        FontChoice::Custom(family) => {
            if let Some(FontChoice::Custom(canonical)) = avail.choice_for(family) {
                fonts.mono = canonical;
            }
        }
        FontChoice::System | FontChoice::Bundled => {}
    }
    fonts
}

#[cfg(test)]
#[path = "../tests/headless/theme_fonts.rs"]
mod tests;
