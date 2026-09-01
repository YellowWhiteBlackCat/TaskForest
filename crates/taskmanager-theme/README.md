# taskmanager-theme

## Role

Toolkit-neutral theme tokens for skins, light/dark mode, high contrast, fonts,
UI size, density, motion and semantic state colors.

## Boundary

The crate owns design facts, not widget layout or OS appearance APIs. Renderer
adapters consume the same tokens; product colors never originate in a page or
literal.

`UiSize` is the desktop readability axis: Small/Standard/Large map the body
baseline to 14/16/18 logical pixels. `FontSize` tokens are authored on the
14px baseline so GPUI can resolve them through its window rem while Iced keeps
the raw values and applies product zoom at the application boundary. Density
owns row whitespace only and must never alter type size.
`UiSize::config_token` and `UiSize::from_config_token` are the sole persisted
token codec; empty, unknown and future tokens resolve to `Standard`.

## Contract and verification

Tokens must cover content, surfaces, borders, focus, selection, hover, warning,
error, unavailable and disabled states with contrast tests. Keep mode/skin
serde compatibility and verify all supported combinations.

The crate is neutral unconditionally (ADR-026, ADR-051): it carries zero
toolkit dependencies and zero features on every target. Each frontend owns its
token→toolkit binding inside its own dependency closure — the GPUI binding in
[`taskmanager-ui::theme_binding`](../taskmanager-ui), the iced binding in
`taskmanager-iced::theme_binding`. Platform compensation (font-weight stem
darkening) is decided once in the neutral `platform` module and projected by
BOTH bindings; its decision table runs on every host, and the theme-neutrality
gate asserts the dependency table names no toolkit. The TUI keeps its lossy
terminal color quantization locally by design (ADR-026).

## Module map

```text
src/tokens.rs  palette.rs  color.rs    neutral layout and color tokens
src/skins.rs                     skin registry — the single color source (ADR-026)
src/theme.rs  fonts.rs           theme assembly and fonts
src/platform.rs  detection.rs    platform and appearance detection
```
