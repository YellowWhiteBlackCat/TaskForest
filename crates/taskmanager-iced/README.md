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
src/a11y.rs                    accessibility bridge
src/app.rs                     IcedApp root
src/app/navigation.rs  input_state.rs  focus_*    navigation, input and focus
src/app/projection.rs  projection_caches.rs      immutable projections and render caches
src/app/refresh.rs  config_sync.rs  configuration_state.rs  preferences.rs
                               refresh scheduling and atomic config/theme substitution
src/app/menus.rs  process_menu.rs  column_menu.rs
src/app/performance_state.rs  alerts.rs  history_*   page state
src/app/affinity.rs  motion.rs  pointer_capture.rs
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
