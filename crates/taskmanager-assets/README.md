# taskmanager-assets

## Role

This crate is the toolkit-neutral source of the TaskForest product identity,
embedded SVG, font bytes, and the shared glass-grain plate.
Every asset is compiled into the consuming binary; frontends decide how to
register or render those bytes. The GPUI `AssetSource` adapter is owned by
`taskmanager-gpui`, while Iced consumes the font bytes directly. The
tray-bitmap and glass-grain regeneration transactions, the `icon_path` bridge
and the `currentColor` rules are specified in
[`docs/ASSET_RULES.md`](docs/ASSET_RULES.md).

## Boundary

This crate embeds bytes only; it does not select a frontend or apply theme
colors, and it does not name a font family: the theme's font collection
(`taskmanager_theme::fonts`) is the only address for a family name, while this
crate owns the registered blobs. The `taskmanager-gates` census
`the_bundled_font_collection_is_the_asset_table` proves the two tables still
describe one fact.

## Contract and verification

Keep asset provenance and `currentColor` behavior stable when changing the
registry. Product-wide asset rules live in
`../../docs/UI_COMPONENT_ARCHITECTURE.md`.

## Module map

```text
src/lib.rs                     deterministic embedded assets (fonts/icons/grain), read-only access
```

## Docs

- [`docs/ASSET_RULES.md`](docs/ASSET_RULES.md) — embedding rules, the tray
  bitmap and glass-grain regeneration transactions, and the `icon_path` bridge.
