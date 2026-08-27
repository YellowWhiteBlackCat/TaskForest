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
