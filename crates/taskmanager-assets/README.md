# taskmanager-assets

## Role

This crate is the toolkit-neutral source of the TaskForest product identity,
embedded SVG, and font bytes.
Every asset is compiled into the consuming binary; frontends decide how to
register or render those bytes. The GPUI `AssetSource` adapter is owned by
`taskmanager-gpui`, while Iced consumes the font bytes directly. The
tray-bitmap regeneration transaction, the `icon_path` bridge and the
`currentColor` rules are specified in
[`docs/ASSET_RULES.md`](docs/ASSET_RULES.md).

## Boundary

This crate embeds bytes only; it does not select a frontend or apply theme
colors.

## Contract and verification

Keep asset provenance and `currentColor` behavior stable when changing the
registry. Product-wide asset rules live in
`../../docs/UI_COMPONENT_ARCHITECTURE.md`.

## Module map

```text
src/lib.rs                     deterministic demo assets (fonts/images), read-only access
```

## Docs

- [`docs/ASSET_RULES.md`](docs/ASSET_RULES.md) — embedding rules, the tray bitmap
  regeneration transaction and the `icon_path` bridge.
