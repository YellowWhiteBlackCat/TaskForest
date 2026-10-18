# Asset rules

Every asset is compiled into the consuming binary; frontends decide how to
register or render those bytes. The GPUI `AssetSource` adapter is owned by
`taskmanager-gpui`, while Iced consumes the font bytes directly.

## Tray bitmap transaction

The product tray bitmap is a decoded RGBA derivative of the dedicated tray
optical master; `packaging/regenerate-icons.sh` regenerates it in the same
transaction as the application ICNS/ICO assets so both desktop frontends
submit identical branded pixels to native tray hosts.

## Icon path bridge

The `icon_path` constants are the stable bridge for semantic `IconId`
mappings. `UI_ICON_PATHS` covers the shared UI glyphs, while
`TASKMANAGER_ICON_PATHS` contains product-domain icons.

UI glyph artwork is monochrome and uses `currentColor`; renderers choose how
to apply those bytes and colors. Product branding preserves its authored
colors. See [ASSET-LICENSE.md](../ASSET-LICENSE.md) for provenance.

The compact window-chrome mark uses `product_small_logo_bytes()`, the same
pixel-aligned 16px optical master used by desktop icon regeneration. Larger
brand placements use `product_logo_bytes()`; neither is a tinted UI glyph.

## Verification

Keep asset provenance and `currentColor` behavior stable when changing the
registry. Product-wide asset rules live in
[`UI_COMPONENT_ARCHITECTURE`](../../../docs/UI_COMPONENT_ARCHITECTURE.md).
