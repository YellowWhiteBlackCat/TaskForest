# ADR-063: Public UI resources have one authority and no frontend fork

Status: accepted (contract, ownership and mechanical gates); the hard-coded
widget-copy census was retired to zero in the same decision line.

## Context

TaskForest ships four product frontends over one typed projection. Three public
resource axes decide whether a user sees the same product in each of them: the
message layer (copy and locale), the theme layer (design tokens) and the icon
layer (semantic identity and assets).

The icon axis already had a single owner with mechanical proof: `IconId`
resolves through one registry into one embedded asset set
(`every_semantic_icon_resolves_to_embedded_svg`,
`the_owner_chain_icons_never_share_one_glyph`).

The theme axis had an owner but not a closed surface. `taskmanager-theme` is
toolkit-neutral (`theme_manifest_has_no_toolkit_dependency_at_all`,
`no_frontend_routes_a_toolkit_through_the_theme`) and owns the token tables, yet
four things still had a second address in the frontends: each product re-spelled
the persisted token of every axis (`"eyeforest" | "eye-forest" =>
LightDark::EyeForest`, `"kde" => Skin::Kde`, `"Compact"`, `"reduced"`, `"none" |
"no-motion"`), each product parsed the shared `TM_SKIN` override again (four
copies), Iced carried a private fourth color-mode variant (`ModeChoice::System`)
next to the theme's `LightDark`, the TUI listed the bundled faces in its own
`FONT_LABELS` table (and offered the monospace face as an interface font),
`taskmanager-assets` re-declared the bundled family names beside the theme's, and
Bevy ignored the saved font preference entirely while its theme snapshot claimed
a host family it cannot load. Numbers survived too: Iced sized the About title
22 and the Performance card title 19/24 off-scale, Bevy seeded
`FontSize::Rem(1.0)`/`0.75` in scenes, and GPUI handed a spinner `18.0`.

The message layer did not. `taskmanager-iced` shipped its own `Language` type,
its own 80-variant key enum and its own hard-coded English/Chinese tables
(437 lines). 41 of those keys already existed in `locales/*.json` and 12 of them
carried different wording, so one product could render "About" while the other
three rendered "About TaskForest" with nothing to compare them. The divergence
was not a toolkit necessity: it was a second address for one fact, which the
charter forbids.

## Decision

1. **The message layer has exactly one Rust address.**
   `locales/{en,zh}.json` is the only copy store,
   `taskmanager-application::i18n` is the only resolver (`t`, `Language`,
   `set_language`, `with_language`, `Language::ALL`, `Language::code`,
   `Language::self_name_key`), and `Language` is the only language type. A
   frontend owns no dictionary, no key enum, no local `Language`, and no
   language-to-token codec.

2. **Copy convergence is a hard cutover, not a migration wait-list.** The Iced
   dictionary is deleted in the same change that moves its copy into the
   catalog. Where the catalog already owned the same meaning, the frontend
   adopts the existing key (`chrome.settings`, `common.export`, `common.confirm`,
   `about.system_information`, `prop.process_details`, `system.export_no_data`,
   `settings.density_comfortable`, `settings.density_compact`,
   `settings.text_subpixel`, `settings.text_grayscale`,
   `settings.startup_remember`, `alerts.manage`) instead of minting a twin. Where
   a string was genuinely frontend-surface copy, it joined the catalog. Divergent
   wording lost: the catalog value is the product value.

3. **Language selection stays explicit where it is state, never where it is
   copy.** A frontend may hold the persisted `Language` value for a picker's
   selected row or for deterministic fixture construction
   (`with_language`); it may not thread a language to resolve text, because the
   resolver already reads the active language. Render helpers therefore lost
   their `language` parameters when those parameters existed only to pick a
   dictionary.

4. **Toolkit-shaped differences remain legal and stay registered.** Terminal
   glyph realization of `IconId` (`ascii_glyph`/`icon_glyph`), lossy terminal
   color quantization, and per-toolkit theme bindings are adapter work over the
   same owner facts, declared in the CORE-04/CORE-08 registries. They are not
   copy forks and this ADR does not flatten them.

5. **The settings and type axes are public resources under the same rule.**
   `taskmanager-theme` owns the axis, its persisted token, and its codec:
   `Skin`/`LightDark`/`ModePreference`/`RowDensity`/`UiSize`/`MotionPolicy`/
   `FontChoice`/`FontPreference` each expose `config_token` plus
   `from_config_token`, `settings::AppearanceOverride` is the single reader of
   the shared `TM_SKIN`/`TM_SKIN_HC` override, `fonts::BUNDLED_FONT_FAMILIES`
   plus `FontRole::bundled_family`/`fallback_families` is the whole font
   collection, and `tokens::FONT_*`/`FONT_WEIGHT_*`/`FONT_RATIO_MICRO`/
   `ICON_INLINE`/`ICON_STANDALONE` is the only size and weight ladder. A frontend
   binds a token to its toolkit; it never spells a family name, a type number, a
   weight, or a saved axis token. Follow-native is a color-scheme *preference*
   (`ModePreference::FollowNative`), so no frontend needs a private mode variant,
   and an unknown token fails closed to the detected value in all four products
   by construction.

6. **The standard is mechanical, with an audited legacy ledger.**
   `tests/logic/quality_gate_test.rs` now enforces:

   | Gate | Rule | Tolerance |
   |---|---|---|
   | `only_the_owner_holds_the_message_layer` | no production source outside the owner embeds a catalog, defines `enum Language`, or spells `Language::En/Zh => "…"` | zero |
   | `no_frontend_paints_chinese_copy_the_catalog_does_not_own` | every Chinese literal in a product tree is a value `locales/zh.json` already owns | zero |
   | `every_i18n_t_callsite_literal_exists_in_the_catalog` | `t("…")` literals resolve in `locales/en.json` | zero |
   | `every_dynamic_i18n_key_producer_resolves_in_both_catalogs` | typed key producers resolve in both catalogs | zero |
   | `english_and_chinese_locale_keys_match_except_explicit_fallback_fixture` | catalogs stay symmetric; placeholders match | zero |
   | `hard_copied_widget_copy_stays_within_its_audited_census` | per-crate count of literals painted at audited render-entry markers | zero on all five render trees |
   | `no_frontend_spells_a_font_family_name` | no production source outside the theme's font collection names a family | zero |
   | `no_frontend_spells_a_hardcoded_type_size` | text is sized only from `tokens::FONT_*`, `UiSize::*_font_size()`, `FONT_RATIO_MICRO` or a palette field derived from them | zero |
   | `no_frontend_spells_a_hardcoded_font_weight` | no numeric weight and no toolkit weight constant outside the theme's ladder and compensation policy | zero |
   | `no_frontend_reimplements_a_theme_axis_codec` | no product tree compares a saved axis token or tables it to an axis variant | zero |
   | `the_bundled_font_collection_is_the_asset_table` | one embedded blob per bundled family, in role order | zero difference |
   | `the_public_axis_codecs_round_trip_and_match_the_wire_tokens` | every axis token round-trips and equals the `taskforest-core` wire constant | zero |

   The census counts only at listed render-entry markers per frontend
   (`.child("/.label(/.placeholder(/.tooltip(`, `text("/button("/titled(`,
   `Span::raw("/Line::from(`, `label: `/`text(`/`title(`), and a new
   copy-emitting API must be added to the marker table with its count in the same
   change. A rise fails the gate; the ledger was retired to zero in the first
   cleanup, so no crate may reintroduce a painted literal. That change retired
   all 27 legacy sites: 12 Iced column identities became stable lowercase tokens
   (the catalog key was already the second `TableColumn::text` argument), seven
   Iced thread/subtree labels and three TUI health hints joined the catalog,
   and five Bevy settings labels/values now resolve through it. The column
   identities were never copy — the audit had counted them as such because they
   occupied a literal slot — so the marker table now counts only painted text.

7. **Documentation states the rule once and links.** The invariant lives in
   `AGENTS.md`; the gate registration lives in `docs/QUALITY_GATES.md`; the
   routing row lives in `docs/README.md`; the owner contract lives in
   `crates/taskmanager-application/README.md`. Root charter files stay inside
   their size caps, so no layer restates this decision as prose parity.

## Consequences

- Adding a locale is a one-place edit in the owner plus one catalog file; no
  frontend can be missed.
- A frontend that wants new copy adds a key to both catalogs and calls `t`;
  forgetting either side fails the quick/standard gates rather than shipping a
  raw key to users.
- Any future product (a fifth frontend, a mobile surface) inherits the same
  gates: it cannot introduce a private dictionary without failing
  `only_the_owner_holds_the_message_layer`.
- Visible text may change when a legacy census site is converted, so each such
  conversion owes the capture receipts its consumer frontend requires.
- A user who set a preference in one product now gets the same palette, density,
  motion limit, size step and faces in the other three, because there is one
  codec; the retired TUI spelling `""` (which meant "host font" there and
  "product default" everywhere else) now resolves to the bundled faces.
- Type sizes converge to the public rungs where a frontend had invented one:
  Iced's About title 22 → `FONT_18` (the GPUI About rung, now header-weighted),
  the Performance card title 19/24 → `FONT_20`/`FONT_26` at
  `FONT_WEIGHT_EXTRA_BOLD` (the GPUI perf-page rung), and its system-information
  value row 14 → `FONT_13` (the GPUI metadata-row rung). Icon/control extents
  (`px(14.0)` on an inline glyph, a column width) stay layout contracts under the
  px firewall; this gate covers type, not geometry.
- Bevy resolves the saved font preference through the shared codec against an
  `FontAvailability::embedded_only` snapshot, so its theme snapshot names the
  faces it can actually paint and never claims a host family it cannot load.
