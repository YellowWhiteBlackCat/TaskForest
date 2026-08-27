# taskmanager-icons

## Role

Shared semantic icon registry with an optional GPUI rendering adapter (ADR-017
Phase 2). Other frontends can consume the same paths and embedded bytes without
enabling the GPUI feature.

`IconId` (the toolkit-neutral semantic identity) lives in
[`taskmanager-ui-contract`](../taskmanager-ui-contract); the embedded tintable
SVG assets live in [`taskmanager-assets`](../taskmanager-assets). This crate
owns the semantic mapping in between:

- [`path`] — resolve an `IconId` to its embedded SVG asset path.
- [`asset_bytes`] — retrieve the embedded SVG bytes for an `IconId`.
- [`icon`] — when the `gpui` feature is enabled, build a GPUI icon element for
  an `IconId`. The glyph inherits the
  surrounding text color (resolved from the text style at layout time) and
  supports the usual GPUI style chain (`.size(..)`, `.text_color(..)`, …).

No `gpui-component` types are used or re-exported.

## Boundary

The registry owns the semantic mapping but does not own product page state or
theme tokens.

## Contract and verification

The registry remains toolkit-neutral until the optional adapter boundary.
Verify every `IconId` has an asset/fallback and that color comes from the
consumer theme; product rules live in
`../../docs/UI_COMPONENT_ARCHITECTURE.md`.
