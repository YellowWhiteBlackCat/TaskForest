# ADR-061: Frosted-glass material system with a REM sizing axis

Status: accepted (extends ADR-020's token governance and ADR-026's toolkit-neutral
theme layer; applies to GPUI, Iced, and Bevy only)

## Context

The product needs one material language across its graphical frontends: translucent
surfaces, a specular edge, soft layered shadow, and an optional noise texture. TUI
is explicitly outside the scope because terminal cells cannot carry a backdrop
filter. The three graphical toolkits expose different capabilities:

- GPUI can request a window-level Mica, Vibrancy, or blurred compositor surface and
  has a pixel-only shadow primitive.
- Iced exposes translucent fills and shadows, but no per-element backdrop filter.
- Bevy exposes translucent fills, border and shadow components, but no general
  per-element backdrop filter.

If each frontend chooses its own fill alpha, edge, radius, or shadow, the same skin
will drift across products and the user's UI-size setting will stop affecting the
material. The repository also requires one typed owner for shared facts and no new
dependency to imitate a native material.

## Decision

1. **The theme owns the material.** `taskmanager-theme` exposes `GlassLayer`
   (`fill`, `edge`, `shadow`), the glass gradient and sheen, and `GlassTier` (`Sidebar`, `Card`, `Modal`,
   `Scrim`). Panel tiers resolve their colors from the active skin and mode;
   `Scrim` resolves the skin's modal depth wash without panel edge, shadow,
   blur, or grain. High contrast forces every panel tier to an opaque fill and
   solid edge while the scrim remains a translucent overlay. Frontends may
   project these values onto toolkit types, but may not invent a second glass
   palette.
   The tier a surface wears is a product decision, not a per-renderer one:
   persistent chrome is `Sidebar`, every floating card/panel/menu/popover
   (action menus, context menus, tooltips, toasts, combobox panels) is `Card`,
   and only true modal surfaces — the confirmation/dialog family — are
   `Modal`. A menu is therefore `Card` on GPUI, Iced, and Bevy alike; it must
   not borrow the modal's stronger elevation on one frontend only.
   A menu also owns its viewport bound: a verb list too long for the space
   around its anchor is capped by a viewport share and scrolled in place
   (GPUI's 360 px body cap states the same rule), so it never flips past a
   window edge and clips its first entries. Bevy's menu scroll body is bounded
   this way, and its capture scenario refuses to emit a frame unless the whole
   card is inside the window.
2. **Material dimensions are root-relative.** Blur radius, specular edge width,
   ambient shadow, and contact shadow are authored as REM values on `GlassTier`.
   `Theme::root_rem_size` carries the live 14/16/18 reading-size step. GPUI and
   Iced set it from `UiSize`; Bevy already supplies the equivalent `RemSize`.
3. **One renderer boundary owns conversion.** GPUI's shadow type accepts pixels
   only, so `taskmanager-ui::theme_binding::glass_shadow` is the sole REM-to-pixel
   conversion for shadows. Layout dimensions continue to use toolkit REM units.
   No glass call site introduces a physical pixel literal.
4. **Capability differences degrade honestly.** GPUI can request its existing
   window-background appearance for Mica/Vibrancy and paints the theme's
   translucent gradient, edge, and two-layer shadow above it. Bevy uses a
   translucent fill, gradient sheen, edge, two-layer shadow, and optional grain. Iced
   paints a translucent gradient and edge and folds the theme's ambient
   and contact layers into its single shadow quad by alpha-weighted geometry;
   it never silently drops the contact layer. Iced and Bevy do not claim a
   per-panel backdrop blur they cannot perform; a gradient is a painted sheen,
   not optical refraction or backdrop blur. Dense rows remain opaque for legibility
   and frame cost.
   Inline chart surfaces keep the card fill and edge but omit elevation
   shadows. Rectangular plot content uses the theme's rounded-content inset:
   a rectangular overflow clip alone cannot protect a rounded corner.
5. **No dependency is added for the material.** The implementation learns the
   visual language only; it uses existing toolkit primitives and theme values.
6. **Noise is capability-scoped, not universal.** `GlassTier::noise_strength`
   is the sole strength authority. Bevy projects it onto the shared
   `taskmanager-assets` grain tile; GPUI and Iced do not paint grain because
   their current element stacks have no equivalent tiled-image layer. This is
   an explicit renderer capability boundary, not a hidden material fork.
7. **Optical glass is a separate capability-gated contract.** The painted
   `GlassLayer` defined here remains the fallback. Backdrop-sampled optical
   glass, renderer capability reporting, fallback ordering and the no-vendor
   rule are governed by [ADR-062](062-backdrop-sampled-optical-glass.md).

## Consequences

- A skin or UI-size change updates every glass surface through one theme snapshot,
  rather than through per-page constants.
- GPUI can request native window translucency where available; all three
  renderers use the shared palette, outline and sheen with their actual
  capabilities; Bevy alone adds the shared grain texture.
- High contrast, light/dark modes, and every skin share the same fallback rules and
  are covered by headless tests plus current-build captures.
- Future work must not treat a glass fill as a dense data row, nor add an
  independent blur/noise package to the graphical products.
- A gradient, shadow, rounded outline or blur-radius declaration does not
  satisfy optical glass. The optical completion label is governed by ADR-062
  and is allowed only when `OpticalGlassDecision::Optical` comes from a
  complete renderer capability; all other outcomes carry a typed fallback
  reason.
- The remaining global-px audit is separate: existing layout contracts retain their
  documented pixel geometry, while new material dimensions must use REM.
