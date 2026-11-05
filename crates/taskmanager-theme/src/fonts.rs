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

/// The complete public font collection: every family the product bundles, in
/// role order (UI, then monospace). This is the ONE address for the bundled
/// family names — a frontend, an asset table, or a settings picker never
/// re-spells `"MiSans VF"` (ADR-063). `taskmanager-assets` owns the registered
/// bytes, and the `taskmanager-gates` census welds one face per role to one
/// embedded blob.
pub const BUNDLED_FONT_FAMILIES: [&'static str; 2] = [FONT_MISANS_VF, FONT_ROBOTO_MONO];

/// The persisted `Config::ui_font` / `Config::mono_font` token that explicitly
/// requests the host's own skin family. The ABSENT (empty) token is not this
/// choice: it means "no recorded preference" and resolves to the bundled
/// product faces, which is why an explicit token exists at all.
pub const FONT_TOKEN_SYSTEM: &str = "system";

/// The two roles a user can pick a face for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FontRole {
    Ui,
    Mono,
}

impl FontRole {
    /// Every role, in the order a picker lists them.
    pub const ALL: [Self; 2] = [Self::Ui, Self::Mono];

    /// The bundled product face this role resolves to when registration was
    /// verified (MiSans VF for the UI role, Roboto Mono for metrics).
    #[must_use]
    pub const fn bundled_family(self) -> &'static str {
        match self {
            Self::Ui => FONT_MISANS_VF,
            Self::Mono => FONT_ROBOTO_MONO,
        }
    }

    /// The installed-family ladder this role falls back to when both the skin
    /// family and the bundled face are absent. Public so a capability probe or
    /// a diagnostics view reads the same order the resolver uses.
    #[must_use]
    pub const fn fallback_families(self) -> &'static [&'static str] {
        match self {
            Self::Ui => &UI_FALLBACK_FAMILIES,
            Self::Mono => &MONO_FALLBACK_FAMILIES,
        }
    }

    /// The literal the resolver uses when the host catalog is empty, that is,
    /// when no candidate in [`Self::fallback_families`] can be observed.
    #[must_use]
    pub const fn empty_catalog_fallback(self) -> &'static str {
        match self {
            Self::Ui => EMPTY_CATALOG_UI_FALLBACK,
            Self::Mono => EMPTY_CATALOG_MONO_FALLBACK,
        }
    }
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

impl FontChoice {
    /// The fixed product ladder a picker offers: the host's own family first,
    /// then the bundled product face (the shipped default). Every frontend uses
    /// this order so a keyboard-navigated picker cannot drift.
    pub const PICKER_LADDER: [Self; 2] = [Self::System, Self::Bundled];

    /// The persisted token for this choice. `Bundled` writes the role's own
    /// bundled family, so a saved config names what the product shipped instead
    /// of an ambiguous marker.
    #[must_use]
    pub const fn config_token(self, role: FontRole) -> &'static str {
        match self {
            Self::System => FONT_TOKEN_SYSTEM,
            Self::Bundled => role.bundled_family(),
            Self::Custom(family) => family,
        }
    }

    /// Resolve a persisted or user-supplied token into a choice with the
    /// product-wide policy every frontend shares: the absent token is the
    /// bundled product default, a token naming a bundled face of this role is
    /// `Bundled`, the explicit system token is `System`, and any other name is
    /// accepted ONLY when the bounded startup catalog observed it. An unobserved
    /// name fails closed to `System`, which resolves to the skin's verified
    /// fallback — a config file can never force a request for a family the host
    /// lacks. The UI role also reads the monospace face's name as a bundled
    /// request (early configs carried it there); the monospace role never treats
    /// the UI face as its own, so one policy still keeps the roles distinct.
    #[must_use]
    pub fn from_config_token(token: &str, role: FontRole, availability: &FontAvailability) -> Self {
        let token = token.trim();
        if token.is_empty() || is_bundled_token_for(token, role) {
            return Self::Bundled;
        }
        if token.eq_ignore_ascii_case(FONT_TOKEN_SYSTEM) {
            return Self::System;
        }
        availability.choice_for(token).unwrap_or(Self::System)
    }
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

impl FontPreference {
    /// The persisted `(ui, mono)` tokens, in configuration-field order.
    #[must_use]
    pub const fn config_tokens(self) -> (&'static str, &'static str) {
        (
            self.ui.config_token(FontRole::Ui),
            self.mono.config_token(FontRole::Mono),
        )
    }

    /// Rebuild a preference from the two persisted tokens through
    /// [`FontChoice::from_config_token`], so every product reads one config file
    /// into the same families.
    #[must_use]
    pub fn from_config_tokens(
        ui_token: &str,
        mono_token: &str,
        availability: &FontAvailability,
    ) -> Self {
        Self {
            ui: FontChoice::from_config_token(ui_token, FontRole::Ui, availability),
            mono: FontChoice::from_config_token(mono_token, FontRole::Mono, availability),
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
            first_installed(
                &names,
                UI_FALLBACK_FAMILIES,
                FontRole::Ui.empty_catalog_fallback(),
            )
        };
        let mono_fallback = if bundled_mono {
            FONT_ROBOTO_MONO
        } else {
            first_installed(
                &names,
                MONO_FALLBACK_FAMILIES,
                FontRole::Mono.empty_catalog_fallback(),
            )
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

    /// The snapshot for a renderer that registers ONLY the embedded faces and
    /// cannot load a host family by name (Bevy's font store consumes asset
    /// bytes, not the platform catalog). Every skin then resolves to the faces
    /// this renderer can actually paint, and no custom family can authorize
    /// itself because no catalog was observed. `embedded_registered` is the
    /// renderer's honest registration result, never an assumption: when it is
    /// `false` both roles report unavailable and resolve through the same
    /// deterministic fallback ladder a missing bundle uses elsewhere.
    #[must_use]
    pub fn embedded_only(embedded_registered: bool) -> Self {
        let resolved = |role: FontRole| {
            if embedded_registered {
                role.bundled_family()
            } else {
                role.empty_catalog_fallback()
            }
        };
        let faces = ResolvedFonts {
            ui: resolved(FontRole::Ui),
            mono: resolved(FontRole::Mono),
        };
        Self {
            per_skin: Skin::ALL.map(|_| faces),
            bundled_ui: embedded_registered,
            bundled_mono: embedded_registered,
            families: Vec::new(),
            custom_families: Vec::new(),
            catalog_truncated: false,
        }
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

/// The literal each role ends on when the toolkit reports no families at all.
/// Reached only through [`FontRole::empty_catalog_fallback`], so the two ladders
/// and this last resort never disagree about which face is the tail.
const EMPTY_CATALOG_UI_FALLBACK: &str = "Noto Sans";
const EMPTY_CATALOG_MONO_FALLBACK: &str = "Noto Sans Mono";

/// Whether a persisted token names a bundled face this role resolves to the
/// [`FontChoice::Bundled`] choice. The UI role also accepts the monospace face's
/// name because early configs carried it in `ui_font`; the monospace role does
/// not accept the UI face, which would silently drop aligned digits.
fn is_bundled_token_for(token: &str, role: FontRole) -> bool {
    token.eq_ignore_ascii_case(role.bundled_family())
        || (matches!(role, FontRole::Ui) && token.eq_ignore_ascii_case(FONT_ROBOTO_MONO))
}

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
