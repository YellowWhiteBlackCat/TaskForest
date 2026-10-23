# taskmanager-shell

## Role

Renderer-neutral state machine shared by GPUI, Iced, TUI, and Bevy (ADR-027): page,
selection, filtering, sorting, effects and `SystemProjectionStore`. Shell is the
composition owner that folds correlated telemetry into the single bounded
`taskmanager-telemetry-store` live graph authority.

## Boundary

Shell folds `PlatformEventBatch` into shared projections but owns no OS I/O,
GPUI Entity, Ratatui widget, Iced widget or native provider selection.

## Key modules

- `src/app.rs` owns the per-track `SystemProjectionStore`; `src/app/batch_fold/`
  folds platform batches through typed phases into shared projections.

## Contract and verification

Cross-crate ownership, batch order and lifecycle matrices are authoritative in
[`docs/STATE_OWNERSHIP.md`](../../docs/STATE_OWNERSHIP.md); this crate only
implements the shell-owned rows of that ledger.

The same projection must drive rendering, keyboard paging, command feedback and
accessibility summaries. Keep frame commit, stale/current semantics and source
notices typed. Frontends request quit with an explicit reason and only read
`should_quit()`; they render feedback through the read-only projection API.
Verify shell behavior independently of a compositor.

## Module map

```text
src/app.rs                          SystemProjectionStore: one instance per frontend track
src/app/batch_fold/                 event folding: failure seed → domain systems → revision
│                                   → alert watermark → feedback
src/app/direct_track/               process inventory, sorting, selection (GPUI track)
src/app/process_rows/               cacheable category/tree structure and row identity
src/app/device_rows.rs              family-qualified device identities and row revisions
src/app/inventory_rows.rs           typed service/startup/session row identity and anchors
src/app/request_sessions.rs         per-track typed request-session instances
src/app/lifecycle.rs                ShellLifecycleState (quit and feedback)
src/app/effects.rs  effect_dispatch.rs     effect generation and dispatch
src/app/fixture_seed.rs             fixture facts enter through shell-owned reducers
src/app/confirmation_gates.rs       dangerous-action confirmation gates
src/app/process_control.rs  process_requests.rs  process action and request handling
src/app/frame.rs  input_mode.rs  selection.rs  search_input.rs
src/app/sorting.rs  sort_axis.rs    sort state and axis definitions
src/app/inventory_source.rs         inventory source selection
src/app/gpu_chart_metric.rs         GPU chart metric session state
src/app/npu_inventory.rs            NPU device inventory projection
src/app/platform_feedback.rs        platform-fold side-effect reducer
src/app/service_log.rs              service log feed lifecycle
src/app/session_control.rs          session control state
src/app/system_telemetry.rs         system telemetry fold
src/app/on_demand.rs  local_keys.rs  row_summary.rs
src/presentation.rs                 renderer-neutral presentation helpers
src/presentation/history_replay.rs  shared metric/device/core replay headings
├── cpu.rs  telemetry.rs  trend.rs  network.rs  storage.rs  process.rs
├── gpu_chart_metric.rs  gpu_engine_rows.rs  service_exit.rs  constants.rs  smart.rs
├── smart_tool.rs        the shared SMART remediation projection (probe + install hint)
├── storage_health.rs    the partition ↔ filesystem health projection
src/viewmodel.rs                    product-first view-model projections
src/history.rs                      correlated outcomes → telemetry store mapping
src/input_dispatch.rs  keys.rs      keyboard dispatch and key definitions
src/process_filter.rs               process filter predicates
src/memory.rs                       memory presentation helpers
src/fixture/ (cpu_topology dashboard_history inventory process_insights setup smbios_memory) deterministic demo/capture/test seam
```

## Docs

- [`docs/BATCH_FOLD.md`](docs/BATCH_FOLD.md) — typestate batch phases, fold ordering,
  feedback lifecycle and inventory failure seeding.
- [`docs/TRACK_OWNERSHIP.md`](docs/TRACK_OWNERSHIP.md) — per-frontend track lifecycles,
  capability snapshots, request sessions, store and alert authority.
- [`docs/PROCESS_ROWS.md`](docs/PROCESS_ROWS.md) — row identity, tree structure and
  process-control availability seams.
- [`docs/PRESENTATION.md`](docs/PRESENTATION.md) — saved views, presentation projections,
  privilege center, GPU chart metric session and the system timeline adapter.
- [`STATE_OWNERSHIP`](../../docs/STATE_OWNERSHIP.md) — cross-crate state ledger;
  [ADR-027](../../adr/027-renderer-independent-shell-state.md) — renderer-independent shell state.

## Shared semantic rows

See [`docs/SEMANTIC_ROWS.md`](docs/SEMANTIC_ROWS.md) for the ordered shell-to-ui-contract adapter, row identity behavior, and typed process measurement handoff.

## Device row identities

See [`docs/DEVICE_ROWS.md`](docs/DEVICE_ROWS.md) for family-qualified identity, lifecycle admission, generation, and row revision behavior.
