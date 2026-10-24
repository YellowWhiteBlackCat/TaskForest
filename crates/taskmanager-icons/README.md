# taskmanager-icons

## Role

Toolkit-neutral semantic icon registry (ADR-017, ADR-051). The crate
compiles zero toolkit code on every target; each frontend materializes icons
in its own crate.

`IconId` (the toolkit-neutral semantic identity) lives in
[`taskmanager-ui-contract`](../taskmanager-ui-contract); the embedded tintable
SVG assets live in [`taskmanager-assets`](../taskmanager-assets). This crate
owns the semantic mapping in between:

- [`path`] — resolve an `IconId` to its embedded SVG asset path.
- [`asset_bytes`] — retrieve the embedded SVG bytes for an `IconId`.

Toolkit rendering adapters are frontend-owned (ADR-051): the GPUI SVG/image
builders live in [`taskmanager-ui::icons_binding`](../taskmanager-ui); the
Bevy raster fallback lives in `taskmanager-bevy-ui`; the iced frontend
resolves the bytes directly. No `gpui-component` types are used or
re-exported anywhere.

## Boundary

The registry owns the semantic mapping but does not own product page state or
theme tokens.

## Contract and verification

The registry is neutral unconditionally — the dependency table names no
toolkit. Neutrality checks and the verification route are specified in
[`docs/MATERIALIZATION.md`](docs/MATERIALIZATION.md).

## Module map

```text
src/path.rs                    IconId → asset path + embedded bytes table
```

IconId → shared SVG → per-toolkit materialization; text glyphs never fake icons.

## Docs

- [`docs/MATERIALIZATION.md`](docs/MATERIALIZATION.md) — the IconId → asset → frontend
  materialization matrix and verification.
- [ADR-017](../../adr/017-own-ui-component-layer.md) and
  [ADR-051](../../adr/051-four-frontend-products-zero-ui-features.md) — owned component
  layer and frontend products.
