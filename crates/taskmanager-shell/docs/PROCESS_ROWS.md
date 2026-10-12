# Process rows and interaction

- `ProcessRowId` distinguishes structural category rows, PID-less application
  aggregates and real process rows. An application root PID is a live tree
  lookup key; batch submission freezes its exact leaf-first descendant scope.
- `ProcessTreeRow` is the sole cacheable category/tree structure: shell owns
  row order, depth, parentage, expansion, identity, and availability-bearing
  group aggregates; frontends only adapt it to their renderer.
- `ProcessControlAvailability` is the sole action-surface projection for
  process-control target scope, exact live count, identity readiness and
  explicit capability failure. `process_control_intent` freezes the matching
  atomic request at this boundary; frontends never rebuild tree, batch or stale
  selection targets.
- `ProcessRowId`/`ProcessRowAnchor` are the row identity seam: process-backed
  rows carry core-owned `ProcessLiveKey` values (the provider-issued process
  incarnation identity), while dangerous control requests freeze a separate
  `FrozenProcessIdentity`. `ProcessProjectionGeneration` rejects stale
  renderer geometry. There is no compatibility row-key API.
- `append_process_semantic_rows` consumes the renderer's exact ordered
  `ProcessTreeRow` slice and appends the category, application, and process
  rows to the `taskmanager-ui-contract` builder. Shell supplies canonical row
  identity, tree state, and source facts; the caller supplies localized row
  names, selection policy, and visible-index lookup. Semantic node IDs use
  `ProcessRowId::stable_key`; expansion keys remain a separate value.
- Identity-missing rows remain visible with generation-and-row-local semantic
  IDs that also include the visible-list index. Those IDs are unstable across
  projections, are not `ProcessLiveKey` values, and expose no focus, selection,
  expand, collapse, or other control actions. Category expansion remains
  structural and has its own stable category key.
- Process scalar observations and all nine `ProcessRowAggregate` metrics pass
  into UI-contract semantic nodes as core-owned `ScalarObservation` and
  `AggregateMetric` values. The handoff preserves availability, failure
  reason, current and last-known values, observation time, and aggregate
  member/current/known coverage. It does not reduce aggregate semantics to
  `current_value`; renderer text formatting remains a frontend responsibility.
- Flat inventory tables use `InventoryRowId` values copied from the owner
  facts `ServiceId`, `StartupEntryId`, or `SessionId`. Missing IDs do not fall
  back to labels, locators, or positions. Stable selection reconciles by ID
  across a reorder; missing or duplicate IDs return no position. An
  `InventoryRowAnchor` separately binds that ID to its table revision and
  active sort, so delayed geometry from a prior data/order projection fails
  closed.
- `DeviceRowId` is the family-qualified device identity for storage disks and
  partitions, network, GPU, power supplies, sensor channels, and NPU rows. A
  physical identity includes its core-owned `DeviceId` and `DeviceGeneration`;
  storage partitions and sensor channels also include their parent identity and
  child ID. Rows require a unique owner and matching application lifecycle
  generation. `DeviceRowAnchor` keeps family data/order revisions separate
  from device incarnation; selection can reconcile by identity after reorder,
  while stale geometry fails closed. A new NPU identity receives its initial
  generation on successful enumeration; reappearance advances it only after a
  successful complete inventory confirms absence. Failed or ambiguous
  inventories do not confirm absence. Alert rows use typed rule, active-instance,
  and event identities with independent shell-owned data/order revisions
  (CORE-14).
- `src/app/selection.rs`, `sorting.rs`, `process_control.rs` and `effects.rs`
  own shared interaction.
