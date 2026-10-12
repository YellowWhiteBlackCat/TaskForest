# taskmanager-theme

## Role

Toolkit-neutral theme tokens for skins, light/dark mode, high contrast, fonts,
UI size, density, motion and semantic state colors.

## Boundary

The crate owns design facts, not widget layout or OS appearance APIs. Renderer
adapters consume the same tokens; product colors never originate in a page or
literal.

The crate is neutral unconditionally (ADR-026, ADR-051): zero toolkit
dependencies and zero features on every target. `UiSize` is the desktop
readability axis; the token codec, baseline rules and neutrality gate live in
[`docs/TOKEN_NEUTRALITY.md`](docs/TOKEN_NEUTRALITY.md).

## Contract and verification

Token coverage, mode/skin serde compatibility and the verification matrix are
specified in [`docs/TOKEN_NEUTRALITY.md`](docs/TOKEN_NEUTRALITY.md).

## Module map

```text
src/tokens.rs  palette.rs  color.rs    neutral layout and color tokens
src/skins.rs                     skin registry — the single color source (ADR-026)
src/theme.rs  fonts.rs           theme assembly and fonts
src/platform.rs  detection.rs    platform and appearance detection
```

## Docs

- [`docs/TOKEN_NEUTRALITY.md`](docs/TOKEN_NEUTRALITY.md) — unconditional neutrality,
  per-frontend bindings, platform compensation and the `UiSize` token codec.
- [ADR-026](../../adr/026-toolkit-neutral-theme-layer.md) — toolkit-neutral theme layer.
