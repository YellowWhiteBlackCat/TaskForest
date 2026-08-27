# ADR-026: Toolkit-neutral theme layer for multiple frontends

Status: accepted

## Context

The application must be able to run as several frontends from one codebase:
the GPUI desktop shell (`crates/taskmanager-gpui/src/gpui_app`), the TUI (`crates/taskmanager-tui`,
ratatui), and a future iced frontend. The non-UI spine already supports this:
`taskmanager-core` → `taskmanager-application` → `taskmanager-ui-contract`
are toolkit-free, and the TUI proves a second frontend can be composed on the
same application layer.

One layer breaks the rule: **`taskmanager-theme` depends on gpui**. Its design
tokens — the 8 skin × 20 color tables (`skins.rs`), the `Palette` contract,
the `Theme` snapshot, the spacing/type/motion tokens — are typed with
`gpui::Rgba`, `Pixels`, `DefiniteLength`, `FontWeight`, `Animation`,
`WindowBackgroundAppearance`, and read window state from `gpui::Window`
(32 gpui references across 7 files). Consequences:

- The TUI cannot consume the skin system; it hardcodes 21 `Color::Rgb(…)`
  literals of its own (crates/taskmanager-tui/src/ui.rs, ui/alerts.rs,
  ui/help.rs), so dark-mode/high-contrast/skin axes do not exist there.
- An iced frontend would have to re-implement or duplicate the whole design
  system; colors, skin registry and detection would drift per frontend.

Design tokens are the one thing every frontend should share: the decision of
*what colors/layouts to use* is presentation policy; only *how to paint them*
is toolkit-specific.

## Decision

Make `taskmanager-theme` toolkit-neutral by default, with the gpui bindings
quarantined behind an optional feature.

### Neutral types (taskmanager-theme, always compiled)

- `Color { r, g, b, a: f32 }` replaces `gpui::Rgba` in `SkinTokens`,
  `Theme`, and `Palette`. `Color` is a plain sRGB RGBA value with const
  constructors (`from_hex`, `with_alpha`, black/white) and the
  luminance/contrast/mix math that the skin system already needs.
- `Length(f32)` (absolute px), `Ratio(f32)` (relative factor) and
  `Weight(f32)` (font weight) replace the gpui-typed token constants
  (`SPACE_*`, `FONT_*`, `LINE_HEIGHT_*`, `FONT_WEIGHT_*`,
  `SELECTION_RAIL`, `Palette::*_radius`, `RowDensity` geometry). The
  values are design data; the unit systems of each toolkit map them.
- Motion durations stay `std::time::Duration` (already neutral).
- `FontAvailability` gains a neutral constructor
  (`from_installed_families(installed: impl Iterator<Item = &str>)`) that
  records an explicitly bounded, deduplicated family catalog plus the
  selectable non-bundled subset. `FontChoice` keeps `Bundled` as the default,
  reserves `System` for the skin's recommended family, and accepts
  `Custom(&'static str)` only for a family observed in that catalog. The
  interner is safe and capped at 2048 names/256 bytes per name; missing or
  stale names fall back to the verified skin default. GPUI supplies the list
  from its text system; Iced uses the mature `fontdb` system scan already
  present in its renderer dependency chain and caches the neutral snapshot.
- Anything inherently gpui — `background_appearance()`,
  `WindowChromeState::from_window`, `Animation` builders — leaves the
  neutral modules. `EdgeTiling`/`WindowChromeState` themselves stay (they
  are plain Copy data, platform-neutral facts).

### Gpui bindings (optional `gpui` feature, one cfg'd module)

Rust's orphan rule (E0117) forbids `impl From<Color> for gpui::Rgba` in any
third crate: both types are foreign there, so the impl may only live in the
crate that defines `Color`. Following the `palette` crate precedent (per-toolkit
conversions behind optional features), `taskmanager-theme` declares
`gpui = { version = "0.2.2", optional = true }` and hosts every gpui binding
in one quarantined module, `src/gpui.rs`, compiled only under
`feature = "gpui"` (off by default):

- `From<Color> for gpui::{Rgba, Hsla, Fill}` — so `.bg(color)` /
  `.text_color(color)` / `.border_color(color)` call sites compile unchanged;
- `From<Length> for gpui::{Pixels, DefiniteLength, AbsoluteLength, Length}`,
  `From<Ratio> for gpui::DefiniteLength` (→ `Fraction`),
  `From<Weight> for gpui::FontWeight`;
- `background_appearance(&Theme) -> gpui::WindowBackgroundAppearance`,
  `window_chrome_state(&gpui::Window) -> WindowChromeState`,
  `detect_font_availability(&gpui::App) -> FontAvailability`;
- `fade_in()` / `appear()` `Animation` builders over the theme durations.

The gpui-side frontend crates (root `taskmanager` package, `taskmanager-ui`)
enable `features = ["gpui"]` on the theme dependency. The TUI and any future
frontend consume the crate with default features off — the theme then has no
toolkit dependency at all. Non-gpui frontends map colors with their own local
helpers (`impl`-free conversions are impossible there too; their call-site
counts are small, e.g. the TUI's single color-mapping module).

### Rules

1. **`taskmanager-theme` has no non-optional toolkit dependency.** A firewall
   test fails any build that makes `gpui`/`ratatui`/`iced` a required
   dependency of the theme, or that enables the optional feature from a
   non-gpui frontend.
2. **The skin registry is the single source of colors.** Frontends may not
   hardcode palettes (the TUI's 21 literals are removed in this ADR's
   migration); they convert `Palette`/`Theme` values to their own color
   type.
3. **Per-frontend widget layers stay per-frontend.** `taskmanager-ui`
   (gpui) and the TUI's ratatui layer keep their own rendering; shared
   contract remains `taskmanager-ui-contract`. The iced frontend builds
   its own widget layer on the same neutral spine + theme.
4. **Layout tokens stay typed.** `Length`/`Ratio`/`Weight` are the token
   types for spacing/type/weight; the `px_literal_gate` contract continues
   to govern raw `px(…)` literals in UI production code.

## Consequences

- **Zero consumer churn** for the common patterns: `.bg(palette.fg)`,
  `.text_size(FONT_13)`, `.gap(SPACE_8)`, `.rounded(palette.panel_radius)`
  all keep compiling because the feature-gated `From` impls exist; the
  204 `text_size(tokens::FONT_*)` call sites and the
  `window_chrome_state`/`background_appearance`/font-detect sites only
  change their call shape, not their location.
- **The theme builds without gpui**: `cargo test -p taskmanager-theme`
  runs the whole neutral suite with the feature off; the TUI consumes the
  same crate with default features off.
- **TUI gains the skin system**: dark/light/high-contrast axes and per-skin
  accents replace the hardcoded terminal palette; terminal mapping stays in
  the TUI (ratatui `Color`), never in the theme crate.
- **iced becomes cheap**: new frontend = a small color-mapping module (its
  own `From`-shaped helpers, since the orphan rule applies there too) +
  widget layer + composition edge; the skin tables, detection and
  application layer are inherited unchanged.
- **One quarantined gpui module**: all toolkit bindings live in
  `taskmanager-theme/src/gpui.rs` behind `feature = "gpui"`; the neutral
  modules never name a toolkit type.
- Current references: `docs/UI_COMPONENT_ARCHITECTURE.md` records the component
  contract; `docs/ARCH.md` records the cross-crate boundary. The superseded
  frontend assessment is private publication material and is not part of the
  current documentation route.
