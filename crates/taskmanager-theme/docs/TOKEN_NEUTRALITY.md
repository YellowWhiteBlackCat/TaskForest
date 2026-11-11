# Token neutrality

The crate is neutral unconditionally (ADR-026, ADR-051): it carries zero
toolkit dependencies and zero features on every target. Each frontend owns
its token→toolkit binding inside its own dependency closure — the GPUI
binding in [`taskmanager-ui::theme_binding`](../../taskmanager-ui), the iced
binding in `taskmanager-iced::theme_binding`.

Platform compensation (font-weight stem darkening) is decided once in the
neutral `platform` module and projected by BOTH bindings; its decision table
runs on every host, and the theme-neutrality gate asserts the dependency
table names no toolkit. The TUI keeps its lossy terminal color quantization
locally by design (ADR-026).

## UiSize codec

`UiSize` is the desktop readability axis: Small/Standard/Large map the body
baseline to 14/16/18 logical pixels. `FontSize` tokens are authored on the
14px baseline so GPUI can resolve them through its window rem while Iced
keeps the raw values and applies product zoom at the application boundary.
Density owns row whitespace only and must never alter type size.

`UiSize::config_token` and `UiSize::from_config_token` are the sole persisted
token codec; empty, unknown and future tokens resolve to `Standard`.

## Axis token codecs (ADR-063)

A saved theme setting is an opaque string in `taskforest-core::Config`; the axis
it names belongs here. Each axis carries its own codec and every frontend calls
it — no product may compare a token or table one by hand:

| Axis | Token out | Token in |
|---|---|---|
| `Skin` | `config_token` (= `label`) | `from_config_token` → `Option` (empty/unknown keeps detection) |
| `LightDark` | `config_token` (= `label`) | `from_config_token` → `Option` |
| `settings::ModePreference` | `config_token` (`System` for follow-native) | `from_config_token` / `from_saved_token` (strict, for a picker that lights no segment) |
| `RowDensity` | `config_token` | `from_config_token` → `Option` (empty = default, unknown ignored) |
| `UiSize` | `config_token` | `from_config_token` (anything unknown → `Standard`) |
| `MotionPolicy` | `config_token` | `from_config_token` (unknown → `Normal`) |
| `FontChoice` / `FontPreference` | `config_token(role)` / `config_tokens` | `from_config_token(role, availability)` / `from_config_tokens` |

`taskforest-core` keeps the same strings as wire constants for the opaque
`Config` fields; `taskmanager-gates` welds the two tables
(`the_public_axis_codecs_round_trip_and_match_the_wire_tokens`) so neither can
drift. `settings::AppearanceOverride` is the single reader of the shared
`TM_SKIN` / `TM_SKIN_HC` capture override, so the harness and the four products
cannot develop different vocabularies.

## Font collection and type ladder (ADR-063)

`fonts::BUNDLED_FONT_FAMILIES` (one face per `FontRole`),
`FontRole::bundled_family`, `Skin::ui_font` / `Skin::mono_font`, and
`FontRole::fallback_families` are the complete public font set: the only place a
family name is spelled. `taskmanager-assets` registers the bytes, never the
names. `FontAvailability::embedded_only` is the snapshot for a renderer that can
load no host family, so its theme names only faces it can paint.

`tokens::FONT_*` is the size ladder, `FONT_BODY`/`FONT_HEADER`/`FONT_CAPTION`
and `UiSize::*_font_size()` the semantic rungs, `FONT_RATIO_MICRO` the one
root-relative micro tier, `FONT_WEIGHT_*` the weight ladder, and
`ICON_INLINE`/`ICON_STANDALONE`/`UiSize::icon_size` the inline extents. Frontends
bind these to toolkit units; they never author a number. The gates
`no_frontend_spells_a_font_family_name`,
`no_frontend_spells_a_hardcoded_type_size` and
`no_frontend_spells_a_hardcoded_font_weight` hold that at zero, with a negative
control each. Icon/control geometry such as a column width or an inline `px` box
stays a layout contract under the px firewall: this charter governs type.

## Layered shadow alpha

The layered glass shadow projection distributes the resolved ink alpha across
its ambient and contact layers in optical depth. Their source-over overlap
therefore stays within the theme's single shadow-opacity budget; the tier's
REM offsets and blur radii remain unchanged. The single-quad projection keeps
its declared elevation reduction.

## Optical glass contract

`GlassLayer` remains the painted frosted fallback. `OpticalGlassSpec` is
separate design intent: blur, refraction, dispersion, specular rim, tint,
frost, grain and corner geometry are authored in REM. Only
`OpticalGlassSpec::resolve` converts them to physical pixels, using the live
root size and device scale factor.

`OpticalGlassCapability` has no implicit default. A renderer must explicitly
report backdrop sampling, blur, refraction, dispersion, specular, grain, the
maximum nesting depth and the physical blur ceiling.
`Theme::optical_glass_decision` is the single fallback authority:

- high contrast always returns `Opaque`;
- headless or no sampleable backdrop returns `Painted`;
- backdrop blur without the complete optical chain returns `Frosted`;
- nesting or blur beyond the renderer limit fails closed to `Frosted` or
  `Painted`.

The locked GPUI and Iced versions report `UNSUPPORTED`. The Bevy P0 proved the
mechanism with public APIs, but its production window remains `UNSUPPORTED`
until the real offscreen material pipeline is integrated. A painted surface is
never relabelled as optical glass.

## Verification

Tokens must cover content, surfaces, borders, focus, selection, hover,
warning, error, unavailable and disabled states with contrast tests. Keep
mode/skin serde compatibility and verify all supported combinations, including
every skin × Light/Dark/EyeForest × high-contrast optical fallback decision.

```bash
cargo nextest run -p taskmanager-theme -j 4
cargo nextest run -p taskmanager-gates --test logic -E 'test(typography_authority_gate_test)' -j 4
```
