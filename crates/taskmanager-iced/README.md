# taskmanager-iced

## Role

Iced desktop frontend with responsive geometry, Canvas charts, semantic focus, keyboard routing and renderer-local
widgets.

## Boundary

Fixed-extent window geometry and original-byte text runs are owned by
`zhuge-ui-core`; generic virtual bodies, rich text, badges and surface styles
are owned by `zhuge-ui-kit-iced`. Product adapters keep column specifications,
search predicates, native theme bindings and business messages. Root workspace dependencies pin the shared Git revision; local development uses
the shared repository’s Cargo patch helper. Each product retains its own lockfile.

It consumes `taskmanager-shell` and toolkit-neutral contracts. It does not access OS sources, choose providers, import
GPUI entities or share large-table widget state with another frontend. Properties resolve the frozen live key against
canonical process facts, independently of table filtering. Overview includes PSS, USS and swap; Performance shows
current and peak values over incarnation-bound curves. Insights selects complete independent facets with fixed controls
around one bounded body; unsupported submissions appear immediately, and Refresh retains the frozen target. Process rows
and Properties consume the app-host-injected local-time rules. Iced owns no `TZ`, zoneinfo filesystem or implicit UTC
fallback path.

## Contract and verification

Cross-crate owners and transition matrices are authoritative in
[`docs/STATE_OWNERSHIP.md`](../../docs/STATE_OWNERSHIP.md); Iced only implements the renderer rows and returns owned
cache snapshots rather than borrow guards.

Preserve compact/wide geometry, current/partial/loading/error projection and Iced-native focus/scroll semantics. Visible
changes require the current Iced matrix and validator; headless changes require behavior/geometry tests.

## Module map

```text
src/main.rs  run.rs            entrypoint and application runner
src/a11y.rs                    accessibility bridge
src/app.rs (+ app/)            IcedApp root, navigation, input_state, refresh, projection
src/ui.rs (+ ui/)              page layouts, modal dialogues, and responsive scaffolds
src/focus.rs (+ focus/)        focus ring management and navigation focus models
src/keys.rs  input_modality.rs keyboard event normalization and input modality state
src/perf_chart.rs  perf_history.rs  app_history_chart.rs  trend_strip.rs
                               canvas charts, historical series and mini sparklines
src/capabilities.rs  functional.rs  feature_coverage.rs
                               parity registry, surface decisions and capability declarations
src/theme.rs  theme_binding.rs  font_catalog.rs  text_metrics.rs
                               palette bindings, typography, font catalog and metrics
src/icons.rs  tray.rs  i18n.rs  icons resolution, system tray and locale helpers
src/saved_views.rs  export.rs  saved view state and tabular data export
src/capture.rs  capture_windows.rs  headless capture and screenshot routines
```

## Docs

- [`docs/APP_STATE.md`](docs/APP_STATE.md) — surface/input scope, update dispatch, alerts
  route, config bridge and sidebar resolution.
- [`docs/CACHES_AND_PROJECTIONS.md`](docs/CACHES_AND_PROJECTIONS.md) — viewport lifetimes,
  renderer memos, store discipline, replay reader and runtime seams.
- [`docs/UI_COMPONENTS.md`](docs/UI_COMPONENTS.md) — component inventory, focus ring,
  virtual list, responsive budgets, appearance, settings and local surfaces.
- [`docs/PARITY_REGISTRY.md`](docs/PARITY_REGISTRY.md) — machine-checked capability parity
  and the page-level divergence registry.
- [`docs/UI_GUIDE_ICED.md`](docs/UI_GUIDE_ICED.md) — Iced widget and rendering guide.
- [`STATE_OWNERSHIP`](../../docs/STATE_OWNERSHIP.md) — cross-crate state ledger;
  [ADR-028](../../adr/028-iced-frontend-three-peer-frontends.md) and
  [ADR-045](../../adr/045-iced-cryoglyph-fixed-lru-floor.md) — Iced frontend and glyph cache.

共享组件扩展与依赖升级遵守 [跨产品共享包契约](../../docs/SHARED_UI_PACKAGES.md)。
