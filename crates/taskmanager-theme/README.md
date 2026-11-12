# taskmanager-theme

## Role

Toolkit-neutral theme tokens for skins, light/dark mode, high contrast, fonts,
UI size, density, motion, semantic state colors, and the four-tier glass
material contract. It is also the single codec between a saved setting and its
axis, the only address for a font family name, and the only ladder for a type
size or weight (ADR-063). `GlassLayer` is the painted frosted fallback; the separate
`optical_glass` contract owns REM-authored optical intent, explicit renderer
capabilities and fail-closed fallback decisions.
The `surface` module owns the semantic glass-surface role to tier/elevation
mapping consumed by the frontend adapters.
`Theme::status_colors` projects the theme's four semantic status inks onto the
shared transient-status ladder, so a frontend resolves a notice's colour
through one owner instead of re-matching the severity itself.
`Theme::glass_fallback_surface` resolves a degraded glass tier into the
material it still wears — fill, edge, shadow, sheen and whether the backdrop
is still sampled — so a renderer that cannot run the optical pipeline never
has to decide for itself what a fallback Card looks like.

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
src/optical_glass.rs                    optical spec, capability and fallback rules
src/surface.rs                          shared glass roles and tier/elevation mapping
src/skins.rs                     skin registry — the single color source (ADR-026)
src/theme.rs  fonts.rs           theme assembly and fonts
src/settings.rs                   saved-axis codecs, mode preference, TM_SKIN override
src/platform.rs  detection.rs    platform and appearance detection
```

## Docs

- [`docs/TOKEN_NEUTRALITY.md`](docs/TOKEN_NEUTRALITY.md) — unconditional neutrality,
  per-frontend bindings, platform compensation and the `UiSize` token codec.
- [ADR-026](../../adr/026-toolkit-neutral-theme-layer.md) — toolkit-neutral theme layer.
- [ADR-061](../../adr/061-frosted-glass-material-system.md) — glass tiers,
  REM-authored material geometry, the scrim/depth-wash contract and the
  capability-gated optical extension.
- [ADR-062](../../adr/062-backdrop-sampled-optical-glass.md) — optical-glass
  definition, backend verdicts, capability reporting and no-vendor rule.
