//! source-inspection: static-policy
//!
//! Typography and theme-settings single-authority gates (ADR-063, the type and
//! settings axes of the public-resource contract).
//!
//! A saved theme setting, a font family, a type size and a font weight are
//! public product resources: four frontends paint one product, so each has one
//! authority — `taskmanager-theme` — and one address for its values. A frontend
//! may bind a token to its toolkit; it may not re-spell, re-number, or
//! re-decode one. These gates are the mechanical form of that rule, and every
//! one of them has zero tolerance plus a negative control proving it is not
//! vacuous.
//!
//! | Gate | Rule |
//! |---|---|
//! | `no_frontend_spells_a_font_family_name` | only the theme's font collection names a family |
//! | `no_frontend_spells_a_hardcoded_type_size` | text is sized from `tokens::FONT_*` / `UiSize::*_font_size()` / a palette field |
//! | `no_frontend_spells_a_hardcoded_font_weight` | text is weighted from `tokens::FONT_WEIGHT_*` |
//! | `no_frontend_reimplements_a_theme_axis_codec` | a saved axis token resolves through the owner's codec |
//! | `the_bundled_font_collection_is_the_asset_table` | one embedded blob per bundled family |
//! | `the_public_axis_codecs_round_trip_and_match_the_wire_tokens` | one token vocabulary, welded to `taskforest-core` |
//!
//! Icon/control extents (`px(14.0)` on an icon node, a column width, a chart
//! dimension) stay layout contracts under the existing px firewall; this gate
//! covers TYPE, not geometry.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use taskmanager_theme::{BUNDLED_FONT_FAMILIES, FontRole, LightDark, ModePreference, Skin};

/// The trees that paint UI: the four products plus the shared component layer.
const RENDER_TREES: [&str; 5] = [
    "crates/taskmanager-gpui/src",
    "crates/taskmanager-iced/src",
    "crates/taskmanager-tui/src",
    "crates/taskmanager-bevy-ui/src",
    "crates/taskmanager-ui/src",
];

/// The files allowed to name a font family: the theme's bundled collection, its
/// fallback policy, and the per-skin system families. Inside the owner these
/// strings ARE the single address; anywhere else they are a fork.
const FAMILY_OWNERS: [&str; 3] = [
    "crates/taskmanager-theme/src/fonts.rs",
    "crates/taskmanager-theme/src/skins.rs",
    "crates/taskmanager-theme/src/theme.rs",
];

/// The theme files allowed to author a numeric weight: the ladder itself and the
/// platform-compensation policy that rounds a rung to a toolkit step.
const WEIGHT_OWNERS: [&str; 2] = [
    "crates/taskmanager-theme/src/tokens.rs",
    "crates/taskmanager-theme/src/platform.rs",
];

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn relative(path: &Path) -> String {
    path.strip_prefix(manifest_dir())
        .unwrap_or(path)
        .display()
        .to_string()
        .replace('\\', "/")
}

fn sources_under(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("source directory must be readable") {
            let path = entry.expect("source entry must be readable").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Every production source tree (`crates/*/src` and the root `src/`).
fn production_sources() -> Vec<PathBuf> {
    let mut trees = vec![manifest_dir().join("src")];
    for entry in fs::read_dir(manifest_dir().join("crates")).expect("crates directory must exist") {
        let src = entry
            .expect("crate entry must be readable")
            .path()
            .join("src");
        if src.is_dir() {
            trees.push(src);
        }
    }
    let mut files = Vec::new();
    for tree in trees {
        files.extend(sources_under(&tree));
    }
    files
}

fn is_comment(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*')
}

/// `(site, line)` for every line of the render trees, where `site` is
/// `path:line`. Comment lines are dropped: prose never compiles, let alone
/// paints, and a gate that reads comments can be talked into silence.
fn render_tree_lines() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for tree in RENDER_TREES {
        let root = manifest_dir().join(tree);
        assert!(
            root.is_dir(),
            "render tree {tree} disappeared; fix RENDER_TREES instead of losing a scan"
        );
        for file in sources_under(&root) {
            for (index, line) in read_lines(&file).into_iter().enumerate() {
                out.push((format!("{}:{}", relative(&file), index + 1), line));
            }
        }
    }
    out
}

fn read_lines(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{} must be readable: {error}", path.display()))
        .lines()
        .map(str::to_owned)
        .collect()
}

/// Every family the product bundles, resolves per skin, or falls back to. The
/// set is derived from the owner's own tables, so a new face is scanned without
/// editing this gate.
fn public_font_families() -> BTreeSet<&'static str> {
    let mut families = BTreeSet::new();
    families.extend(BUNDLED_FONT_FAMILIES);
    for skin in Skin::ALL {
        families.insert(skin.ui_font());
        families.insert(skin.mono_font());
    }
    for role in FontRole::ALL {
        for family in role.fallback_families() {
            families.insert(family);
        }
        families.insert(role.empty_catalog_fallback());
    }
    families
}

#[test]
fn no_frontend_spells_a_font_family_name() {
    let families = public_font_families();
    let mut findings = Vec::new();
    let mut files = 0usize;
    for path in production_sources() {
        let site = relative(&path);
        if FAMILY_OWNERS.contains(&site.as_str()) {
            continue;
        }
        files += 1;
        for (index, line) in read_lines(&path).into_iter().enumerate() {
            if is_comment(&line) {
                continue;
            }
            for family in &families {
                if line.contains(&format!("\"{family}\"")) {
                    findings.push(format!("{site}:{}: names the family {family:?}", index + 1));
                }
            }
        }
    }
    assert!(
        files > 100,
        "the family scan reached only {files} production files, so the tree walk is broken"
    );
    assert!(
        !families.is_empty(),
        "the public font collection resolved empty, so the owner API disappeared"
    );
    assert!(
        findings.is_empty(),
        "a font family name has exactly one address: the theme's font collection \
         (`BUNDLED_FONT_FAMILIES`, `FontRole::bundled_family`, `Skin::ui_font`/`mono_font`, \
         `FontRole::fallback_families`). A second spelling lets one product drift:\n{}",
        findings.join("\n")
    );
}

/// One typography slot: a name whose value must come from the public scale.
///
/// `px_is_number` separates TYPE from EXTENT. In a type slot (`font_size:`)
/// `px(13.0)` is an authored size, so `px(` is transparent and the number still
/// counts as a literal. In the widget extent slot (`.size(`) the same call is an
/// icon/control extent — a layout contract the px firewall governs — so `px(`
/// closes the slot and the line stays legal.
struct Slot {
    name: &'static str,
    px_is_number: bool,
}

const TYPE_SLOTS: [Slot; 12] = [
    Slot {
        name: ".size",
        px_is_number: false,
    },
    Slot {
        name: "text_size",
        px_is_number: true,
    },
    Slot {
        name: "font_size",
        px_is_number: true,
    },
    Slot {
        name: "FontSize",
        px_is_number: true,
    },
    Slot {
        name: "line_height",
        px_is_number: true,
    },
    Slot {
        name: "title_size",
        px_is_number: true,
    },
    Slot {
        name: "subtitle_size",
        px_is_number: true,
    },
    Slot {
        name: "heading_size",
        px_is_number: true,
    },
    Slot {
        name: "caption_size",
        px_is_number: true,
    },
    Slot {
        name: "label_size",
        px_is_number: true,
    },
    Slot {
        name: "value_size",
        px_is_number: true,
    },
    Slot {
        name: "body_size",
        px_is_number: true,
    },
];

const WEIGHT_SLOTS: [Slot; 5] = [
    Slot {
        name: "Weight",
        px_is_number: true,
    },
    Slot {
        name: "FontWeight",
        px_is_number: true,
    },
    Slot {
        name: "font_weight",
        px_is_number: true,
    },
    Slot {
        name: ".weight",
        px_is_number: true,
    },
    Slot {
        name: "weight",
        px_is_number: true,
    },
];

/// The calls that carry a token or a conversion instead of an authored number.
const TOKEN_WRAPPERS: [&str; 6] = [
    "f32::from(",
    "absolute(",
    "rem(",
    "font_size(",
    "FontSize::Rem(",
    "FontSize::Px(",
];

/// The characters that open a slot's value after its name.
const SLOT_OPENERS: [&str; 3] = ["(", ":", "="];

/// Whether `haystack` fills `slot` with a bare number.
///
/// An occurrence preceded by an identifier character belongs to a longer name
/// (`body_font_size`, `flex_weight`) and is skipped, so a geometry or an
/// unrelated field is never read as typography.
fn slot_holds_a_literal(haystack: &str, slot: &Slot) -> bool {
    let mut from = 0usize;
    while let Some(at) = haystack[from..].find(slot.name) {
        let absolute = from + at;
        let boundary_ok = absolute == 0
            || !haystack[..absolute]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_');
        if boundary_ok {
            let after = &haystack[absolute + slot.name.len()..];
            let after = after.trim_start_matches([' ', '\t']);
            let opened = SLOT_OPENERS
                .iter()
                .find(|opener| after.starts_with(**opener))
                .map(|opener| &after[opener.len()..]);
            if let Some(mut tail) = opened {
                loop {
                    let trimmed = tail.trim_start_matches([' ', '\t', '\n', '\r']);
                    let wrapper = TOKEN_WRAPPERS
                        .iter()
                        .find(|open| trimmed.starts_with(**open))
                        .or_else(|| {
                            slot.px_is_number
                                .then_some(&"px(")
                                .filter(|open| trimmed.starts_with(**open))
                        });
                    match wrapper {
                        Some(open) => {
                            tail = &trimmed[open.len()..];
                            continue;
                        }
                        None => {
                            let tail = trimmed.strip_prefix('-').unwrap_or(trimmed);
                            if tail.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                                return true;
                            }
                            break;
                        }
                    }
                }
            }
        }
        from = absolute + slot.name.len();
    }
    false
}

/// The name of the type slot a line fills with a literal, if any.
fn hardcoded_type_size_in(line: &str) -> Option<&'static str> {
    TYPE_SLOTS
        .iter()
        .find(|slot| slot_holds_a_literal(line, slot))
        .map(|slot| slot.name)
}

/// The name of the weight slot a line fills with a literal, if any. A toolkit
/// weight constant (`FontWeight::Bold`) is a second ladder of its own.
fn hardcoded_font_weight_in(line: &str) -> Option<&'static str> {
    if let Some(slot) = WEIGHT_SLOTS
        .iter()
        .find(|slot| slot_holds_a_literal(line, slot))
    {
        return Some(slot.name);
    }
    if line.contains("FontWeight::") {
        return Some("FontWeight::");
    }
    None
}

/// Negative control: the matcher reads real call shapes and spares token calls.
#[test]
fn type_size_scan_detects_a_literal_and_spares_a_token() {
    // The four real call shapes a frontend uses to size text.
    assert_eq!(
        hardcoded_type_size_in("text(name).size(22),").map(str::to_owned),
        Some(".size".to_owned())
    );
    assert_eq!(
        hardcoded_type_size_in("title_size: 19.0,").map(str::to_owned),
        Some("title_size".to_owned())
    );
    assert_eq!(
        hardcoded_type_size_in("let size = FontSize(13.0);").map(str::to_owned),
        Some("FontSize".to_owned())
    );
    assert_eq!(
        hardcoded_type_size_in("text_size(px(13.0))").map(str::to_owned),
        Some("text_size".to_owned())
    );
    // Tokens, conversions, and an icon extent stay legal.
    assert!(hardcoded_type_size_in("text_size(font_size(tokens::FONT_13))").is_none());
    assert!(hardcoded_type_size_in("text(name).size(f32::from(tokens::FONT_BODY)))").is_none());
    assert!(hardcoded_type_size_in(".size(f32::from(tokens::ICON_STANDALONE))").is_none());
    assert!(hardcoded_type_size_in("icon(id).size(px(14.0))").is_none());
    assert!(hardcoded_type_size_in("width: px(240.0)").is_none());
    assert!(hardcoded_type_size_in("font_size: FontSize::Rem(ratio)").is_none());
    assert!(hardcoded_type_size_in("body_font_size() * 2.0").is_none());
    // An assignment slot is the same literal in a different shape.
    assert_eq!(
        hardcoded_type_size_in("let font_size = 11.0;").map(str::to_owned),
        Some("font_size".to_owned())
    );
    assert!(hardcoded_type_size_in("let font_size = window.text_style().font_size;").is_none());
}

#[test]
fn no_frontend_spells_a_hardcoded_type_size() {
    let mut findings = Vec::new();
    let mut lines_scanned = 0usize;
    for (site, line) in render_tree_lines() {
        if is_comment(&line) {
            continue;
        }
        lines_scanned += 1;
        if let Some(marker) = hardcoded_type_size_in(&line) {
            findings.push(format!(
                "{site}: {marker} — {}",
                line.trim().chars().take(120).collect::<String>()
            ));
        }
    }
    assert!(
        lines_scanned > 20_000,
        "the type scan reached {lines_scanned} lines across five render trees, which means the \
         walk is broken"
    );
    assert!(
        findings.is_empty(),
        "type sizes come from the public scale: `tokens::FONT_*`, `UiSize::*_font_size()`, \
         `tokens::FONT_RATIO_MICRO`, or a palette field built from them. A literal is a rung no \
         other product paints:\n{}",
        findings.join("\n")
    );
}

/// The owner axis enums and their exact variant sets. A consumer that
/// redeclares the same variants under its own name has forked the axis,
/// whether or not it re-numbers the values.
const AXIS_VARIANT_SETS: [(&str, &[&str]); 5] = [
    ("UiSize", &["Small", "Standard", "Large"]),
    ("RowDensity", &["Comfortable", "Compact"]),
    ("MotionPolicy", &["Normal", "Reduced", "NoMotion"]),
    ("ModePreference", &["FollowNative", "Explicit"]),
    ("Skin", &["Gnome", "Kde", "Windows", "Macos"]),
];

/// The tree that owns the axis vocabulary. Any enum declared inside the theme
/// crate IS the owner declaration; anywhere else it is a fork.
const AXIS_VARIANT_OWNER_PREFIX: &str = "crates/taskmanager-theme/src/";

fn axis_mirror_in_file(path: &Path) -> Option<(&'static str, &'static str)> {
    let site = relative(path);
    if site.starts_with(AXIS_VARIANT_OWNER_PREFIX) {
        return None;
    }
    let text = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{} must be readable: {error}", path.display()));
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if !(trimmed.starts_with("pub enum ") || trimmed.starts_with("enum ")) {
            continue;
        }
        // The name is only a lookup key; the returned axis comes from the
        // static table, so nothing borrowed from this file escapes.
        let declared_name = trimmed
            .trim_start_matches("pub ")
            .trim_start_matches("enum ")
            .split(['{', '(', '<'])
            .next()
            .unwrap_or_default()
            .trim();
        let Some((axis, variants)) = AXIS_VARIANT_SETS
            .iter()
            .find(|(axis, _)| *axis == declared_name)
        else {
            continue;
        };
        let mut body = String::new();
        let mut depth = 1usize;
        for next in lines.by_ref() {
            let next_trimmed = next.trim();
            if next_trimmed.starts_with('}') {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            } else if next_trimmed.starts_with('{') {
                depth += 1;
            }
            body.push_str(next_trimmed);
            body.push('\n');
        }
        let declared: Vec<&str> = body
            .split([',', '\n'])
            .map(|item| item.trim())
            .filter(|item| {
                !item.is_empty()
                    && !item.contains('(')
                    && !item.contains('{')
                    && !item.contains(':')
                    && !item.contains('=')
                    && item
                        .chars()
                        .next()
                        .is_some_and(|first| first.is_ascii_uppercase())
            })
            .collect();
        if declared.len() == variants.len()
            && variants
                .iter()
                .all(|variant| declared.iter().any(|item| item == variant))
        {
            return Some((axis, "redeclares a theme axis enum"));
        }
    }
    None
}

/// Negative control: the mirror detector really fires on a fork and spares the
/// owner tree.
#[test]
fn axis_mirror_scan_detects_a_fork_and_spares_the_owner() {
    let owner = manifest_dir().join("crates/taskmanager-theme/src/tokens.rs");
    assert!(axis_mirror_in_file(&owner).is_none());

    // A fork: same variants, a second address. A minimal file keeps the
    // negative control pointing at exactly the mirror, not at the owner's own
    // declarations.
    let fork_file = manifest_dir().join("target/axis-mirror-negative-control.rs");
    fs::create_dir_all(fork_file.parent().unwrap()).unwrap();
    fs::write(
        &fork_file,
        "pub enum UiSize {\n    Small,\n    Standard,\n    Large,\n}\n",
    )
    .unwrap();
    assert!(axis_mirror_in_file(&fork_file).is_some());
    fs::remove_file(&fork_file).unwrap();
}

#[test]
fn no_consumer_mirrors_a_theme_axis_enum() {
    let mut findings = Vec::new();
    let mut files = 0usize;
    for path in production_sources() {
        files += 1;
        if let Some((axis, reason)) = axis_mirror_in_file(&path) {
            findings.push(format!("{}: {axis} {reason}", relative(&path)));
        }
    }
    assert!(
        files > 100,
        "the axis-mirror scan reached {files} production files; the walk is broken"
    );
    assert!(
        findings.is_empty(),
        "a theme axis has one declaration, in `taskmanager-theme`. A consumer that redeclares \
         the same variants forks the axis and can drift from the other three products:\n{}",
        findings.join("\n")
    );
}

#[test]
fn no_frontend_spells_a_hardcoded_font_weight() {
    let mut findings = Vec::new();
    let mut files = 0usize;
    for path in production_sources() {
        let site = relative(&path);
        if WEIGHT_OWNERS.contains(&site.as_str()) {
            continue;
        }
        files += 1;
        for (index, line) in read_lines(&path).into_iter().enumerate() {
            if is_comment(&line) {
                continue;
            }
            if let Some(marker) = hardcoded_font_weight_in(&line) {
                findings.push(format!(
                    "{site}:{}: {marker} — {}",
                    index + 1,
                    line.trim().chars().take(120).collect::<String>()
                ));
            }
        }
    }
    assert!(
        files > 100,
        "the weight scan reached only {files} files, so the tree walk is broken"
    );
    assert!(
        findings.is_empty(),
        "weights come from the theme's public ladder (`tokens::FONT_WEIGHT_*`), which the \
         platform-compensation policy then maps per toolkit. A number or a toolkit weight \
         constant re-forks the axis:\n{}",
        findings.join("\n")
    );
}

#[test]
fn weight_scan_detects_a_literal_and_spares_a_token() {
    assert_eq!(
        hardcoded_font_weight_in("Weight(450.0)").map(str::to_owned),
        Some("Weight".to_owned())
    );
    assert_eq!(
        hardcoded_font_weight_in("let w = FontWeight(600);").map(str::to_owned),
        Some("FontWeight".to_owned())
    );
    assert_eq!(
        hardcoded_font_weight_in("weight: FontWeight::Bold,").map(str::to_owned),
        Some("FontWeight::".to_owned())
    );
    assert!(hardcoded_font_weight_in("font_weight(tokens::FONT_WEIGHT_HEADER)").is_none());
    assert!(hardcoded_font_weight_in("weight: theme_weight(tokens::FONT_WEIGHT_BODY)").is_none());
    assert!(hardcoded_font_weight_in(".weight(Weight(500.0))").is_some());
}

/// The persisted tokens of every theme axis, canonical spelling plus the
/// accepted aliases. A frontend that compares or tables one of these has
/// re-implemented the axis codec.
const AXIS_TOKENS: [&str; 22] = [
    "Light",
    "Dark",
    "EyeForest",
    "eyeforest",
    "eye-forest",
    "System",
    "system",
    "GNOME",
    "gnome",
    "KDE",
    "kde",
    "Windows",
    "windows",
    "win",
    "macOS",
    "macos",
    "mac",
    "Comfortable",
    "Compact",
    "normal",
    "reduced",
    "no-motion",
];

/// The axis types a private token table resolves to.
const AXIS_TYPES: [&str; 8] = [
    "Skin::",
    "LightDark::",
    "RowDensity::",
    "MotionPolicy::",
    "UiSize::",
    "FontChoice::",
    "ModePreference::",
    "HighContrast::",
];

fn axis_codec_fork_in_line(line: &str) -> Option<&'static str> {
    for token in AXIS_TOKENS {
        if line.contains(&format!("eq_ignore_ascii_case(\"{token}\")"))
            || line.contains(&format!("== \"{token}\""))
            || line.contains(&format!("\"{token}\" =="))
        {
            return Some("compares a persisted theme token by hand");
        }
        let arm = format!("\"{token}\" =>");
        if let Some(position) = line.find(&arm) {
            let tail = line[position + arm.len()..].trim_start();
            if AXIS_TYPES
                .iter()
                .any(|axis| tail.starts_with(&format!("Some({axis}")) || tail.starts_with(axis))
            {
                return Some("spells a token-to-axis table");
            }
        }
    }
    None
}

#[test]
fn no_frontend_reimplements_a_theme_axis_codec() {
    let mut findings = Vec::new();
    for (site, line) in render_tree_lines() {
        if is_comment(&line) {
            continue;
        }
        if let Some(reason) = axis_codec_fork_in_line(&line) {
            findings.push(format!("{site}: {reason}: {}", line.trim()));
        }
    }
    assert!(
        findings.is_empty(),
        "a saved theme token resolves through the theme owner's codec \
         (`Skin`/`LightDark`/`ModePreference`/`RowDensity`/`UiSize`/`MotionPolicy`/`FontChoice` \
         `config_token` + `from_config_token`). A frontend that matches one by hand can drift \
         from the other three products:\n{}",
        findings.join("\n")
    );
}

#[test]
fn axis_codec_scan_detects_a_private_table() {
    assert_eq!(
        axis_codec_fork_in_line("if config.mode.eq_ignore_ascii_case(\"System\") {"),
        Some("compares a persisted theme token by hand")
    );
    assert_eq!(
        axis_codec_fork_in_line("\"kde\" => Some(Skin::Kde),"),
        Some("spells a token-to-axis table")
    );
    assert_eq!(
        axis_codec_fork_in_line("\"reduced\" => MotionPolicy::Reduced,"),
        Some("spells a token-to-axis table")
    );
    // Non-axis vocabulary stays legal: process names, OS names, catalog keys.
    assert_eq!(
        axis_codec_fork_in_line("\"System\" => Some(i18n::t(\"page.system\"))"),
        None
    );
    assert_eq!(
        axis_codec_fork_in_line("TopPage::System => \"system\","),
        None
    );
    assert_eq!(
        axis_codec_fork_in_line("let mode = ModePreference::from_config_token(t);"),
        None
    );
}

#[test]
fn the_bundled_font_collection_is_the_asset_table() {
    use taskmanager_assets::embedded_fonts;
    // One registered blob per bundled family: the bytes and the names are two
    // halves of one fact and cannot drift.
    assert_eq!(
        embedded_fonts().len(),
        BUNDLED_FONT_FAMILIES.len(),
        "the embedded font table and the public font collection disagree"
    );
    assert_eq!(
        BUNDLED_FONT_FAMILIES,
        FontRole::ALL.map(FontRole::bundled_family)
    );
}

#[test]
fn the_public_axis_codecs_round_trip_and_match_the_wire_tokens() {
    use taskforest_core::config::{
        COLOR_SCHEME_DARK, COLOR_SCHEME_EYEFOREST, COLOR_SCHEME_LIGHT, COLOR_SCHEME_SYSTEM,
        DENSITY_COMFORTABLE, DENSITY_COMPACT, MOTION_NONE, MOTION_NORMAL, MOTION_REDUCED,
    };
    use taskmanager_theme::tokens::{MotionPolicy, RowDensity, UiSize};

    // The theme's tokens ARE the configuration's wire vocabulary.
    assert_eq!(LightDark::Light.config_token(), COLOR_SCHEME_LIGHT);
    assert_eq!(LightDark::Dark.config_token(), COLOR_SCHEME_DARK);
    assert_eq!(LightDark::EyeForest.config_token(), COLOR_SCHEME_EYEFOREST);
    assert_eq!(
        ModePreference::FollowNative.config_token(),
        COLOR_SCHEME_SYSTEM
    );
    assert_eq!(RowDensity::Comfortable.config_token(), DENSITY_COMFORTABLE);
    assert_eq!(RowDensity::Compact.config_token(), DENSITY_COMPACT);
    assert_eq!(MotionPolicy::Normal.config_token(), MOTION_NORMAL);
    assert_eq!(MotionPolicy::Reduced.config_token(), MOTION_REDUCED);
    assert_eq!(MotionPolicy::NoMotion.config_token(), MOTION_NONE);

    for skin in Skin::ALL {
        assert_eq!(Skin::from_config_token(skin.config_token()), Some(skin));
    }
    for mode in LightDark::ALL {
        assert_eq!(
            ModePreference::from_config_token(mode.config_token()),
            ModePreference::Explicit(mode)
        );
    }
    for density in RowDensity::ALL {
        assert_eq!(
            RowDensity::from_config_token(density.config_token()),
            Some(density)
        );
    }
    for size in UiSize::ALL {
        assert_eq!(UiSize::from_config_token(size.config_token()), size);
        assert_eq!(size.config_token(), size.config_token());
    }
    for policy in MotionPolicy::ALL {
        assert_eq!(
            MotionPolicy::from_config_token(policy.config_token()),
            policy
        );
    }
    // The picker ladder is exactly the codec's vocabulary, in one published
    // order, so a settings surface cannot light a segment the store cannot hold.
    for choice in ModePreference::CHOICES {
        assert_eq!(
            ModePreference::from_saved_token(choice.config_token()),
            Some(choice)
        );
    }

    // Fail-closed, in every product: an unknown token never forces a palette,
    // a geometry, a size, or a font.
    assert_eq!(Skin::from_config_token("cinnamon"), None);
    assert_eq!(
        ModePreference::from_config_token("sepia"),
        ModePreference::FollowNative
    );
    assert_eq!(RowDensity::from_config_token("roomy"), None);
    assert_eq!(
        MotionPolicy::from_config_token("cinematic"),
        MotionPolicy::Normal
    );
    assert_eq!(UiSize::from_config_token("huge"), UiSize::Standard);
}

#[test]
fn font_choice_tokens_round_trip_for_every_role() {
    use taskmanager_theme::{FontAvailability, FontChoice, FontPreference};
    let availability =
        FontAvailability::from_installed_families([BUNDLED_FONT_FAMILIES[0], "Fira Sans"]);
    for role in FontRole::ALL {
        for choice in FontChoice::PICKER_LADDER {
            let token = choice.config_token(role);
            assert_eq!(
                FontChoice::from_config_token(token, role, &availability),
                choice,
                "{role:?} {choice:?} must survive its own token {token:?}"
            );
        }
    }
    let preference = FontPreference {
        ui: FontChoice::Custom("Fira Sans"),
        mono: FontChoice::System,
    };
    let (ui, mono) = preference.config_tokens();
    assert_eq!(
        FontPreference::from_config_tokens(ui, mono, &availability),
        preference
    );
}

#[test]
fn the_public_type_scale_is_ordered_and_axis_aware() {
    use taskmanager_theme::Length;
    use taskmanager_theme::tokens::{self, UiSize};

    for size in UiSize::ALL {
        assert!(
            size.caption_font_size() < size.header_font_size(),
            "{size:?} must keep the header above the caption"
        );
        assert!(size.header_font_size() < size.body_font_size());
        assert!(size.body_font_size() < size.section_title_font_size());
        assert!(size.section_title_font_size() < size.page_title_font_size());
        // A rem renderer resolves the body rung to exactly one root unit.
        assert_eq!(f32::from(size.rem_ratio(size.body_font_size())), 1.0);
        assert_eq!(f32::from(size.rem_ratio(Length(0.0))), 1.0);
        // Every display rung binds the SAME tighter leading, so a page title
        // never buys its own box out of the vertical budget under it.
        assert_eq!(size.display_line_height(), tokens::LINE_HEIGHT_DISPLAY);
    }
    assert!(
        tokens::LINE_HEIGHT_DISPLAY < tokens::LINE_HEIGHT_HEADER,
        "large type takes tighter leading, never looser"
    );
    assert!(tokens::LINE_HEIGHT_DISPLAY < tokens::LINE_HEIGHT_NORMAL);
    assert!(tokens::FONT_WEIGHT_NORMAL < tokens::FONT_WEIGHT_BODY);
    assert!(tokens::FONT_WEIGHT_BODY < tokens::FONT_WEIGHT_MEDIUM);
    assert!(tokens::FONT_WEIGHT_MEDIUM < tokens::FONT_WEIGHT_HEADER);
    assert!(tokens::FONT_WEIGHT_HEADER < tokens::FONT_WEIGHT_STRONG);
    assert!(tokens::FONT_WEIGHT_STRONG < tokens::FONT_WEIGHT_EXTRA_BOLD);
    assert_eq!(tokens::FONT_WEIGHT_SEMIBOLD, tokens::FONT_WEIGHT_HEADER);
    assert_eq!(tokens::FONT_WEIGHT_BOLD, tokens::FONT_WEIGHT_STRONG);
    // The ladder and the semantic roles agree, so two call sites that name
    // different tokens cannot paint different numbers.
    assert_eq!(f32::from(tokens::FONT_BODY), 14.0);
    assert_eq!(f32::from(tokens::FONT_HEADER), 12.0);
    assert_eq!(f32::from(tokens::FONT_CAPTION), 10.0);
    assert!(f32::from(tokens::FONT_RATIO_MICRO) > 0.0);
}
