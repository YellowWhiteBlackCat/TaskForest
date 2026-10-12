# Icon materialization

`IconId` (the toolkit-neutral semantic identity) lives in
[`taskmanager-ui-contract`](../../taskmanager-ui-contract); the embedded
tintable SVG assets live in [`taskmanager-assets`](../../taskmanager-assets).
This crate owns the semantic mapping in between:

- [`path`](../src/path.rs) — resolve an `IconId` to its embedded SVG asset
  path.
- [`asset_bytes`](../src/path.rs) — retrieve the embedded SVG bytes for an
  `IconId`.

Toolkit rendering adapters are frontend-owned (ADR-051):

| Frontend | Materialization |
|---|---|
| GPUI | SVG/image builders in `taskmanager-ui::icons_binding` |
| Bevy | raster fallback in `taskmanager-bevy-ui` |
| Iced | resolves the bytes directly |
| TUI | no icon surface; text glyphs never fake icons |

No `gpui-component` types are used or re-exported anywhere.

## Verification

The registry is neutral unconditionally — the dependency table names no
toolkit. Verify every `IconId` has an asset/fallback and that color comes
from the consumer theme; product rules live in
[`UI_COMPONENT_ARCHITECTURE`](../../../docs/UI_COMPONENT_ARCHITECTURE.md).

```bash
cargo nextest run -p taskmanager-icons -j 4
```
