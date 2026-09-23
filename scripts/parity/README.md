# Cross-frontend evidence manifest (P4 pilot)

This directory carries the P4 "evidence closure" pilot: a single committed
declaration manifest plus a resolver that checks every declared behavior anchor
against real test discovery.

- `cross_frontend_manifest.tsv` — the declaration list. One row per
  `(subject_kind, subject_id, frontend)`.
- `feature_evidence.tsv` — the P5 feature-level evidence table (G2 closure).
  One row per `(feature_id, frontend)`: the hand-declared nextest anchor that
  backs that shape's delivery claim, or an explicit `pending` gap. It is the
  data source the Rust gate's evidence closure reads through
  `include_str!`, so a `Ready` cell can no longer be claimed from the static
  source commitment alone.
- `feature_evidence_co_anchors.tsv` — the sparse optional column of the table
  above (W23-B). One row per extra hand-declared test that proves a named
  clause of an already-anchored `(feature_id, frontend)` cell when the primary
  anchor test does not drive that clause's production surface (the registered
  cases are the iced and bevy ordering-cycle clauses). The primary `test_id`
  keeps its meaning and stays the single anchor authority; every `co_test_id`
  is resolved against the owning frontend's discovery exactly like a primary
  anchor, and the table can never make a cell `Ready` (the Rust G2 closure
  reads `feature_evidence.tsv` alone). It is a side table rather than a sixth
  column because the Rust consumer
  (`crates/taskmanager-ui-contract/src/feature_coverage/platform_gate/evidence.rs`)
  embeds `feature_evidence.tsv` with exactly five columns, so appending one is
  a hard schema cutover that must change `crates/**` in the same commit; until
  that window the column is materialized sparsely here, keyed by the cell it
  extends.
- `accept-frontend-interactions.sh` / `accept_frontend_interactions.py` — the
  unified S5 driver. Since W23-B the `gpui-interactions` and
  `bevy-interactions` stages of `scripts/quality/local-gates.sh` run it; per
  selected frontend it records the frontend-scoped source fingerprint, invokes
  the legacy `scripts/accept-<frontend>-interactions.sh` gate unchanged (still
  the authoritative per-frontend runner and receipt writer), folds the fresh
  receipts into one run segment, re-runs this resolver over the same discovery,
  and verifies the aggregate cross-frontend run manifest.
- `cross_frontend_matrix.tsv` — the unified interaction matrix (S4 + W10-B). It
  carries the per-frontend interaction matrices as one list with a `frontend`
  dimension, including the **TUI rows that never had a per-frontend matrix**
  (anchors declared by hand against real `cargo nextest list` discovery; cases
  without a discoverable anchor are marked `pending`). The original
  `scripts/{gpui,iced,bevy}_interaction_matrix.tsv` stay committed as
  **compatibility views** (read by the existing accept scripts and validators)
  until the S5 retirement wave moves those callers.
- `resolve_frontend_evidence.py` — Layer B resolver. It compares declared
  `behavior` anchors with `cargo nextest list` output and fails on dangling
  anchors (R4) or target/frontend mismatches (R6). With `--interaction-matrix`
  it consumes the unified matrix as a second declaration source, including
  `pending` rows, with `--feature-evidence` (default:
  `scripts/parity/feature_evidence.tsv`, so it is on in the gate) the P5
  feature-level table as a third, and with `--co-anchors` (default:
  `scripts/parity/feature_evidence_co_anchors.tsv`) the sparse co-anchor side
  table in the same pass. It owns no contract-tag vocabulary
  (`contract_tag` is an opaque required field), no requirement vocabulary
  (`p0_id` is opaque unless `--requirements` supplies the public id list), and
  no feature vocabulary (`feature_id` is opaque; the Rust registry owns it).
  `--scope auto` is the local-gate entry: it short-circuits when the git diff
  since `--base` cannot move an anchor (see "Local gate route"), and
  `--report-json` pins the machine-readable report path.
- `test_resolve_frontend_evidence.py` — standard-library self-test proving the
  resolver rejects a deleted referenced test, a target mismatch, a duplicate
  cell, malformed declarations, malformed unified-matrix rows, a deleted
  interaction anchor on either channel, a deleted feature-level anchor, and a
  deleted feature co-anchor; counts `pending` rows instead of dangling them,
  validates requirement coverage fail-closed, rejects a co-anchor that names an
  unanchored cell or repeats its primary anchor, and only skips the diff-scope
  when no evidence-relevant path changed (fail-closed on a failed diff probe).

The contract vocabulary itself is the Rust
[`ContractTag`](../../crates/taskmanager-ui-contract/src/conformance.rs) enum.
Its conformance test reads `cross_frontend_manifest.tsv` (every `contract_tag`)
and `cross_frontend_matrix.tsv` (every `contract_tag` and every `paths` token)
and fails on any unknown id, so the vocabulary has exactly one authority (Rust)
and both committed declarations only reference its ids.

## Manifest columns

| column | meaning |
|---|---|
| `subject_kind` | `facet` in this pilot; schema reserves `intent`, `capability`, `feature` |
| `subject_id` | stable subject id |
| `frontend` | `gpui`, `iced`, `tui`, or `bevy` |
| `status` | `ready`, `partial`, `missing`, `unsupported`, or `pending` |
| `reason` | required for `partial`/`missing`/`unsupported` |
| `contract_tag` | which contract property the anchor proves |
| `evidence_kind` | `behavior`, `visual`, `none`, or `pending` |
| `test_id_or_scenario` | nextest test id (behavior) or capture scenario (visual) |
| `target_or_validator` | frontend that owns the anchor (must equal `frontend` for behavior) |
| `platform` | **reserved P5 axis**; empty = frontend axis. The column exists so the three-dimensional ledger can land without a schema break. It is opaque here (like `contract_tag`), carries no vocabulary yet, and every committed row leaves it empty. |

`pending` means "no discoverable anchor has been assigned yet". Pending cells
are counted and reported, never treated as dangling.

## Feature-level evidence table (P5 G2 closure)

`feature_evidence.tsv` is the P4 closure of the P5 three-axis ledger's G2 rule:
a `(feature, frontend)` cell may claim `Ready` only when a real, discoverable
behaviour test backs that shape's delivery claim.

| column | meaning |
|---|---|
| `feature_id` | stable `FeatureId` id. The vocabulary authority is the Rust `FeatureId::ALL` registry in `taskmanager-ui-contract`; the contract test parses this file and rejects an unknown id, so no second vocabulary exists here. |
| `frontend` | `gpui`, `iced`, `tui`, or `bevy` |
| `test_id` | the nextest test path exactly as discovery reports it, or `-` on a pending row |
| `status` | `anchored` or `pending` |
| `note` | `-` on an anchored row; the honest gap on a pending row |

Semantics:

- **anchored** — the row names a hand-declared test id; the resolver checks set
  membership against the owning frontend's discovery (R4), so a renamed or
  deleted test is `dangling` and fails the run. The id is never derived by
  scanning Rust source.
- **pending** — the pair was surveyed and no discoverable behaviour test exists
  yet. The row is counted, never dangling, and never becomes an anchor: the
  folded cell stays refused by G2, with the note as the recorded gap.
- **absent** — the pair has not been surveyed yet. Absence is not a delivery
  claim either: `Ready` still requires a committed anchor.

The Rust side reads the same file through `include_str!`
(`crates/taskmanager-ui-contract/src/feature_coverage/platform_gate/evidence.rs`)
and folds it with `PlatformGatePolicy::evidence_closure`, which attaches the
anchor only where the feature's whole `Requires` capability set is `Present` on
that platform axis. That keeps G4 by construction (a `Missing`/`Unsupported`/`Gated`
cell can never carry an anchor) while G2 is untouched: a `Ready` cell without a
committed anchor is still a finding, so the table cannot be used to make a cell
`Ready`; it can only document a real test.

Anchored batches (2026-09-22):

- First batch (W13-A): `handles.enumeration`, `threads.topology`,
  `services.lifecycle-control`, and `services.dependency-dag`, each anchored on
  all four frontends (16 rows), plus two surveyed `pending` gaps on Bevy
  (`storage.smart-health`, `storage.swap-throughput`).
- Second batch (W14-A): eight further features whose delivered surface exists on
  the frontends below and whose whole `Requires` capability set is `Present` on
  the Linux source lane - `gpu.engine-utilization` (gpui/iced/tui),
  `memory.breakdown-rss-pss` (gpui/iced/tui), `storage.device-topology`
  (iced/tui), `services.inventory` (iced/tui), `security.posix-capabilities`
  (tui), `security.sandbox-detection` (bevy),
  `hardware.heterogeneous-cores` (gpui/tui), and `hardware.core-frequency`
  (tui) - 15 rows. Anchors are partial by design: a frontend stays unanchored
  where no test proves the shape's delivered surface, and four further
  surveyed near-misses are recorded as `pending` (GPUI services-inventory and
  device-topology, Bevy device-topology and engine-utilization).
- Third batch (W15-A): six further cells whose whole `Requires` capability set
  is `Present` on the Linux source lane and whose delivered surface a real
  frame/panel test proves - `storage.smart-health` (iced/tui, SMART
  temperature + endurance + power-on rows), `storage.iops-queue-latency`
  (tui, IOPS + latency rows), `power.battery-inventory` (iced/tui, charge +
  power + voltage + cycle rows with honest absence), and `services.log-stream`
  (iced, the panel's own resolved stream lines plus the level-filter control
  reaching the shared query). Each anchor was re-read clause by clause against
  the feature's `delivery_definition`; the clauses that stay unasserted are
  named in the `pending` notes of the same survey (SMART spare/unsafe
  shutdowns, the queue-depth row, battery health, thermal-zone source
  traversal, the process-details fault and huge-page counters, the GPUI
  battery/SMART/log surfaces, and the Bevy namespace audit). The fifth batch
  below closes several of those recorded clauses (queue depth, thermal-zone
  source traversal, the process-details counters, the Bevy namespace audit and
  log stream, the Bevy SMART family); each remaining `pending` note still
  names its own live gap.
  The survey added `pending` rows only where a real near-miss exists; a
  feature with no test touching its surface stays without a row.
- Fourth batch (W16-A): the two GPUI cells whose `pending` notes rested on
  test-tree-only fixtures. `crates/taskmanager-gpui/tests/gui/gpui_app/system_view/detail_rows.rs`
  used to define `battery_detail_rows` / `thermal_control_rows` inside the test
  tree; no production System-page code ever rendered them (their i18n keys had
  zero production references), so the five tests that folded them proved a
  surface that does not exist. The fixture and those tests were deleted; the
  GPUI battery and thermal-zone claims are now anchored on the shape's real
  delivery surfaces - the Performance page battery panel
  (`perf_views::dynamic_stats::tests`, charge + power + voltage + health +
  cycle rows with honest absence) and the System Health sensor center
  (`system_health_view::stats::tests`, one labelled row per thermal-zone
  reading with its real value and typed absence). The fixture's thermal-control
  half (`system.cooling` / `system.throttle`) matches
  `power.thermal-throttle-events`, which this shape declared `Unsupported` at
  that point, and gained no row then — the W25-A batch below later delivered
  and anchored that cell on the real CPU counters surface.
- Fifth batch (W16-B/W16-C): eighteen new anchored rows plus one re-verified
  row, each hand-checked clause by clause against the feature's
  `delivery_definition` and each test id confirmed by membership in
  `cargo nextest list` before it was committed. W16-B closed the TUI
  observation gaps - the Properties-modal page-fault counters, the
  details-panel anonymous huge-page charge, the painted service log stream
  whose rows follow the shared level filter, and the disk panel's
  queue/service row (the already-anchored TUI `storage.iops-queue-latency`
  row is the re-verified one: the same frame test now writes the
  queue/service pair explicitly and proves all three clauses - IOPS, latency
  and queue depth - in one frame). W16-C closed the Iced/Bevy observations -
  swap-in/out throughput rates, IOPS/response/queue/service rows, the
  PageFaults/AnonHugePages counters, per-adapter GPU enumeration (Iced and
  Bevy) with Bevy's per-engine rows, Bevy partition rows and the six-family
  Bevy SMART evidence (availability, temperature, percentage used, available
  spare, power-on hours, unsafe shutdowns), the Bevy painted log stream under
  its level filter, and the Bevy Linux namespace audit. Five cells had no row
  before and were newly surveyed
  (Iced `gpu.adapter-enumeration`; Bevy `storage.iops-queue-latency`,
  `gpu.adapter-enumeration`, `memory.page-faults`,
  `memory.transparent-huge-pages`); the other thirteen replace a surveyed
  `pending` row with the verified anchor. The TUI `storage.iops-queue-latency`
  row keeps `-` in its note column (the schema reserves notes for pending
  rows), so this paragraph carries its clause record. The same batch gained a
  nineteenth anchor in W17-A: the Iced health line landed the real
  `power.thermal-zones` delivery surface (the health modal's thermal-zone
  panel) and the Iced cell moved from `pending` to the traversal test - one
  row per shared sensor-center temperature reading, each named by the
  reading's own source label, with a failed read kept as the shared dash and
  never a fabricated `0.0 °C`. The same module's panel test (panel appears for
  a temperature channel, absent for a fan-only snapshot) and capture-fixture
  test are supporting evidence, not the anchor.
- Sixth batch (2026-09-22, W18-A): the six GPUI cells the previous surveys
  left as explicit `pending` gaps, each hand-checked clause by clause against
  the feature's `delivery_definition` and each test id confirmed by membership
  in `cargo nextest list -p taskmanager-gpui --features test-support`.
  `services.inventory` anchors the painted inventory frame (one row per
  projected unit; each row's status cell carries its typed `ServiceStatus`
  token; the typed status filter is the painted membership authority and an
  empty match paints no placeholder row). `storage.device-topology` anchors
  the disk page's painted partition rows (identity/usage/bar slots, the fill
  width equal to the observed used/total fraction, the unmounted unit kept as
  the compact summary line, and no fill at all for an unobserved partition).
  `storage.smart-health` anchors the production `disk_stats` fold on all six
  definition families (availability, temperature with its critical bound and
  per-sensor rows, percentage used, available spare with its threshold,
  power-on hours, unsafe shutdowns) with the unobserved provider growing none
  of them. `storage.swap-throughput` anchors the Memory page's stat fold on
  the real swap-in/swap-out rates (the used/total occupancy row is a separate
  fact; an unobserved or typed-unavailable rate leaves no row).
  `storage.iops-queue-latency` anchors the same fold on IOPS 137, response
  1.54 ms, average queue depth 2.25, and the separate service-time estimate,
  with a first-sample gap kept as the shared dash. `services.log-stream`
  anchors the painted service-details dialog: the accepted batch paints one
  row per projected entry and advancing the painted level control
  (All → Errors) before the next batch leaves only the error row painted,
  while a cold or rejected stream paints the typed state and no fabricated
  row. Two GPUI observation limits are recorded with this batch: the harness
  exposes geometry per debug selector (no text readback), so the inventory
  status-cell selector derives its token from the same typed `ServiceStatus`
  the cell paints; and a `debug_bounds` entry survives the frame that painted
  it, so states that must be absent are asserted on a fresh window rather than
  as an in-window disappearance. A follow-up in the same batch converted the
  TUI `storage.swap-throughput` cell after the TUI line landed its
  painted-frame test (`Swap in 2.0 MiB/s` and `Swap out 512.0 KiB/s` on one
  frame through the canonical memory scalar group; a `TimedOut` observation
  keeps both labelled rows on the shared dash, with an explicit
  no-fabricated-`0 B/s` assertion). The closing follow-up (W20 tail) then
  converted the table's last surveyed gap: the TUI `power.thermal-zones` cell
  is anchored on the Fan page's SYSTEM traversal after the TUI line landed
  `thermal_zone_lines` - one painted row per shared temperature reading,
  including a foreign-device zone the device-level fan context cannot reach,
  each named by the reading's own source label, with an unread channel kept
  as the named shared dash and the frame asserted to carry no fabricated
  `0.0 °C`. Its sibling tests (whole-group admission, fanless reachability)
  are supporting evidence, not the anchor.

- W23-A batch (2026-09-23): the `memory.breakdown-rss-pss` sweep.
  The P1 `delivery_definition` was narrowed to the facets the shared memory
  projection really delivers — resident (RSS), proportional (PSS),
  private/unique (USS), and the derived-shared share (`RSS - USS`) — with the
  virtual address-space size explicitly OUTSIDE it (the shared projection
  carries no `VmSize` observation; `memory.vma-map` owns that area) and the
  per-process swap charge kept as a separate fact. The application VM gained
  `ProcessDetailsField::Shared` and all four details surfaces paint it, so
  every anchor proves the complete narrowed definition: the GPUI row was
  re-pointed from the gates-package table projection
  (`processes_view_test::memory_projection_prefers_current_pss_and_falls_back_to_typed_rss`)
  to the in-crate details-dialog fold
  (`gpui_app::root::chrome::tests::memory_breakdown_rows_render_every_narrowed_facet`:
  the resident performance current plus the PSS/USS/derived-shared overview
  rows, with the derived share keeping the shared dash when USS is
  unobserved); the Iced and TUI anchors gained the derived-shared value
  (`768.0 MiB` from `RSS - USS` in the same typed observation family) and the
  cold-facet dash clause; and the Bevy cell — declared `Ported` but
  previously unanchored — gained its first anchor on the details-overview
  fold.

- W25-A batch (2026-09-23): the `power.thermal-throttle-events` sweep. The
  W23-A definition had already been narrowed to the cumulative trigger
  counters (`package_throttle_count` / `core_throttle_count` of the shared
  CPU projection), with the real-time PROCHOT `is_throttled` state explicitly
  OUTSIDE it; W24-A then made `CpuPackageMetrics` their single authority and
  registered the `telemetry.cpu.throttle` lane (`Present` on Linux,
  registered-pending typed `Unsupported` on Windows/macOS). All four shapes
  now really render the counters on their CPU details surfaces, so the four
  declared `Unsupported` cells move to their honest delivery status (GPUI
  `Reference`; Iced, TUI, and Bevy `Ported`) and each cell gains its anchor.
  Every anchor proves the complete narrowed definition: one
  `S{package_id}` segment per package that observed at least one counter,
  both the package-level and the per-core event count painted from the shared
  projection, the unobserved-family honest absence (GPUI's spec list, the
  Iced stat column, and the TUI rail omit the row; Bevy's always-mounted
  diagnostic row keeps the shared dash), and the labeled shared dash for an
  unobserved sibling counter — never a fabricated `0`. The PROCHOT clause
  stays deliberately unasserted (it is outside the definition), and the
  Linux-only `telemetry.cpu.throttle` lane keeps each cell host-derived.
  This takes the table to 71 anchored rows (gpui 16, iced 18, tui 20,
  bevy 17).
- W28-A batch (2026-09-23): the `memory.process-swap-charge` ownership
  addition. The per-process swap charge is a real typed fact every shape
  renders in its Apps-table Swap column; it was unclaimed because
  `memory.breakdown-rss-pss` explicitly excludes it and `storage.swap-throughput`
  is system-level. The feature anchors Iced's
  `apps_resource_projection_preserves_typed_pss_swap_and_measured_zero` and
  TUI's `apps_table_projects_typed_pss_and_swap_without_zero_fallbacks`; GPUI
  and Bevy first landed as explicit `pending` gaps because their nearest tests
  proved only the column auto-hide rule and the honest dash.
- W28-P batch (2026-09-23): the two `memory.process-swap-charge` gaps were
  closed with real tests rather than relabelled — GPUI
  `swap_cell_renders_the_observed_per_process_charge_without_a_zero_fallback`
  and Bevy `row_view_renders_the_observed_per_process_swap_charge` both feed an
  observed `swap_bytes` and assert the rendered charge (plus the dash for an
  unobserved one), so all four shapes now anchor the cell.

The table now carries **75 anchored + 0 `pending`** rows (per frontend: gpui 17,
iced 19, tui 21, bevy 18 anchored). Every registered cell is committed
evidence; a new near-miss must again be recorded as an explicit `pending` row.
Every other source-complete cell keeps its G2 finding until a real test is
anchored; the batches are a bounded delivery, never a blanket `Ready` claim. A
`pending` row is a survey record, not a delivery claim, and it never becomes an
anchor.

### Feature co-anchors (`feature_evidence_co_anchors.tsv`, W23-B)

The W23-B batch is not a new cell but a schema extension: `feature_evidence.tsv`
can now record a **co-anchor**, a further discoverable test that proves a named
clause of an already-anchored cell whose production surface the primary anchor
test does not drive. The table is read in the same resolver pass
(`--co-anchors`, default on, fail closed when the file is missing).

| column | meaning |
|---|---|
| `feature_id` | stable `FeatureId` id; it must name an **anchored** `(feature_id, frontend)` row of `feature_evidence.tsv`, whose primary `test_id` keeps its meaning |
| `frontend` | `gpui`, `iced`, `tui`, or `bevy` |
| `co_test_id` | the extra nextest test path, exactly as discovery reports it for the owning frontend |
| `reason` | the delivery-definition clause the co-anchor proves (never `-`) |

Rules, all fail-closed in the same resolver pass:

- **dangling** - a `co_test_id` that discovery no longer lists fails the run
  with its cell, frontend, test id and the `feature-evidence-co-anchor` channel,
  exactly like a deleted primary anchor (R4);
- **invalid** - a row whose `(feature_id, frontend)` cell has no anchored
  feature-evidence row, or whose `co_test_id` repeats that cell's primary
  anchor, fails the run (the primary row already resolves it);
- the primary anchor grid is untouched: `feature_evidence.anchored` and
  `feature_evidence.dangling` do not move when a co-anchor is added, so the
  Rust G2 closure and its `Pending`/`Anchored` semantics are unchanged;
- the report block is `co_anchors: {path, rows, cells, dangling}` in the
  `--json`/`--report-json` output, plus
  `counts.feature_co_anchors` / `feature_co_anchor_cells` /
  `feature_co_anchor_dangling`; the shared `counts.dangling` total includes the
  co-anchor share and the human summary prints a `co-anch.:` line when any row
  is declared.

The registered batch (W23-B, two rows, each hand-verified against
`cargo nextest list`):

- `services.dependency-dag` / iced - the ordering-cycle clause is co-proved by
  `ui::tests::pages::service_projection_preserves_fixture_rows_and_typed_status`
  (the `services.inventory` anchor test): both members of a typed `Before`-cycle
  carry the shared cycle flag the row paints;
- `services.dependency-dag` / bevy - the same clause is co-proved by
  `pages::services::tests::ordering_cycle_members_paint_the_warning_plate`:
  each cycle-member row paints the Alert plate while an acyclic row stays
  unmarked (the dependency panel itself does not paint the cycle, which is why
  the primary anchor does not drive it).

A third candidate - the GPUI `memory.breakdown-rss-pss` private (USS) clause
via the dialog's overview mirror test - was withdrawn in the same window: the
W23-A memory sweep re-pointed that cell's primary anchor to the sibling
in-crate dialog test `memory_breakdown_rows_render_every_narrowed_facet`, which
proves the resident/proportional/private/derived-shared clauses together, so the
candidate no longer proves an undriven clause and keeping it would add a
redundant dangling surface. A co-anchor is never a substitute for re-pointing a
primary anchor, and the table must never become an anchor authority of its own:
it carries no status, no `Ready` claim, and no vocabulary. When a `crates/**`
window can change the Rust consumer of `feature_evidence.tsv`, the side table
is the natural source of a sixth column folded into that file in one cutover
(see "Known S4/S5 residuals").

## Unified interaction matrix

`cross_frontend_matrix.tsv` is the S4 unified list (extended by W10-B with the
`tui` block): one row per `(frontend, case_id)`, mechanically transcribed from
the per-frontend matrices, with anchors declared by hand and verified by set
membership against `cargo nextest list`. Nothing was derived by scanning Rust
source.

| column | meaning |
|---|---|
| `subject_kind` | `interaction` (the new subject type; the schema reserves room for future `requirement` rows) |
| `case_id` | stable case id from the owning frontend's matrix |
| `frontend` | `gpui`, `iced`, `tui`, or `bevy` — the new dimension |
| `p0_id` | public requirement id from `scripts/interaction_requirements.tsv`; `-` where no mapping is declared (26 Bevy rows retain `-` under the D1 partial mapping + reasoned exemption) |
| `target` | `gui` or `lib` (matrix semantics, unchanged) |
| `test_name` | nextest test path; `-` when the row uses a stable case-prefix channel; `pending` when the case is declared but no discoverable anchor exists yet |
| `paths` | ordered behavior/contract paths (matrix semantics, unchanged) |
| `capture_scenarios` | `-` or `|`-joined capture scenario names (matrix semantics, unchanged) |
| `contract_tag` | the row's primary contract tag: it must equal the first `paths` token. Opaque to the resolver, whose only check is that matrix-internal equality; the vocabulary authority stays the Rust `ContractTag` enum. |
| `platform` | **reserved P5 axis**, same discipline as the manifest: empty in every committed row and opaque to the resolver. |

The TUI block (47 rows, all anchored) is the D2 deliverable. TUI has no stable
case-prefix convention and no per-frontend matrix, so every row names its test
explicitly. The last two D2 gaps were closed by real keyboard ports rather than
by relabelling: the terminal shape has no pointer drag or hover surface (its
capability registry declares `ColumnDragResize` and `Tooltip` unsupported — "no
pointer-driven column-edge drag surface", "no hover surface" — and the runtime
deliberately drops drag/move events as unmodeled), so a UI-parity line ported
the two gestures and gave each its own case id (`mc03-tui-column-reorder` = the
column menu's `←`/`→` reorder, `mc05-tui-chart-cursor` = the Performance·CPU
chart's `←`/`→` sample cursor). Both carry `success|keyboard`, not the source
case's stale `pointer`: a keyboard reorder is not a drag and a keyboard sample
cursor is not a hover. The other two D2 gaps were closed earlier by real tests,
not relabelled: `mc02-tui-hotplug` anchors the storage-family fail-closed
fallback and `mc07-tui-capture-visual` anchors the supervised capture frame's
typed marker (see "Known S4/S5 residuals"). All 47 rows declare a `P0-MC-*` id;
together with GPUI/Iced they cover all eight requirements on three frontends.
Bevy covers four of the eight (see "Bevy `p0_id`" below).

Anchor-source recognition in the resolver (`--interaction-matrix PATH`):

- rows with an explicit `test_name` resolve by exact membership in the owning
  frontend's discovery (iced/bevy/tui rows);
- rows with `test_name = -` use the owning frontend's **stable case-prefix
  channel** — accepted only for frontends listed in `CASE_PREFIX_CHANNELS`
  (GPUI: `<case_id>` with `-` normalized to `_`, plus `_case_`), mirroring the
  existing GPUI validator;
- rows with `test_name = pending` declare a case whose anchor is not assigned
  yet: they are counted in `interaction_matrix.pending` and in the shared
  `pending` list, never treated as dangling, and can never cover a requirement;
- anchored rows missing from discovery are `dangling` (R4) and fail the run;
  the dangling entry names the frontend, case id, requirement id and test id.

Consumption in the gate is wired since W11-C: `scripts/quality/local-gates.sh`
`standard` calls the resolver with `--interaction-matrix
scripts/parity/cross_frontend_matrix.tsv`, so the unified matrix is a second
fail-closed declaration source in the `parity-evidence` stage (a renamed or
deleted interaction anchor is `dangling`). The old per-frontend matrices stay
authoritative for the accept scripts until the S5 retirement wave moves those
callers; the gate no longer waits for that wave to check the unified list.

### Transition semantics before S5 (D3/D4)

Two unified-matrix conventions are declared here as the accepted **transition
semantics** for the window before the S5 hard cutover (the private convergence
plan owns the schedule). This section documents the current default; it does
not change the resolver's flags, defaults, or checks.

- **`contract_tag = paths[0]` (D4).** A row's `contract_tag` is its primary
  contract tag and must equal the first `paths` token. The resolver checks only
  that matrix-internal equality; it never defines, validates, or copies the tag
  vocabulary. The single authority stays the Rust `ContractTag` enum, so the
  rule adds no second address for tags. It is accepted as the primary-label rule
  for the transition window. The nine Bevy path tokens were folded into the enum
  by the D5 change, and the enum's conformance test validates this file's
  `contract_tag` and `paths` columns, so every id the rule compares is now
  Rust-owned. A review that wants a different priority (for example "channel
  first") must decide before S5 deletes the compatibility views; the equality
  rule survives into S5.
- **GPUI stable case-prefix channel (D3).** A GPUI row may declare
  `test_name = -`; its anchor then resolves through the prefix `<case_id>` with
  `-` normalized to `_`, plus `_case_`, matched against the function name of any
  discovered test. The channel is open only to frontends listed in
  `CASE_PREFIX_CHANNELS` (currently GPUI alone) and mirrors the legacy GPUI
  validator's guarantee, so the unified matrix is never weaker than the
  per-frontend gate it will replace. It cannot see a rename that preserves the
  prefix. The owner may instead backfill an explicit `test_name` in the 39 GPUI
  rows; that costs one hand-verified discovery pass and must never be filled by
  scanning Rust source. Either way the resolver semantics stay the same; the
  choice is recorded as an owner decision in the private register (D3).

### Requirement coverage (`--requirements`, `--require-requirement-coverage`)

`scripts/interaction_requirements.tsv` is the public `P0-MC-*` id vocabulary
(already read by the GPUI/Iced matrix validators). Passing `--requirements PATH`
(or `--require-requirement-coverage`, which defaults to that file) makes the
resolver

- reject an interaction `p0_id` that is neither `-` nor a declared requirement,
- report `requirement_coverage` per frontend (covered = at least one anchored
  row for that frontend declares the id **and** carries the `success` path
  token — the per-frontend generalization of the GPUI validator rule),
- with `--require-requirement-coverage`, fail closed on every uncovered
  `(frontend, requirement)` pair.

Without either flag `p0_id` stays opaque and the coverage block is `null`, so
existing callers are unaffected. This is the mechanism the D1 resolution below
uses; it does not decide the mapping.

The `parity-evidence` gate stage passes `--requirements` and deliberately omits
`--require-requirement-coverage`. So the stage records the per-frontend coverage
report and rejects an unknown `p0_id`; the fail-closed flag stays deferred
because the D1 resolution maps Bevy 4/8 and declares `P0-MC-01/02/04/05` as a
reasoned exemption (below), so the flag would report exactly those four exempt
`(frontend, requirement)` pairs. Release condition: the flag lands when the four
exempt Bevy pairs gain real success-path anchors and this matrix declares them
(or the resolver gains an explicit exemption channel), at which point the
requirement axis becomes a fail-closed 8/8 x 4 check.

### Bevy `p0_id` (D1 resolved: partial mapping + reasoned exemption)

D1 is resolved on 2026-09-23 as a **partial hand-declared mapping with a
reasoned exemption**, not a fabricated 8/8. Each of the 52 Bevy rows was read
against its anchor's real assertions; 26 declare the requirement that anchor
genuinely covers and the other 26 keep `-`. Bevy therefore covers **4 of the 8**
`P0-MC-*` requirements (0 dangling, 26 unmapped cells).

Mapped (anchors read clause by clause, never inferred from the case id):

- **P0-MC-00** (shared-page navigation/routing over the frozen page set):
  `bev-route-keyboard` (Alt+1 through the shared router moves the route and
  remounts exactly one page under the slot), `bev-route-programmatic`
  (`RouteChanged` observer chain remounts the new page), `bev-route-history`
  (the shared page chords resolve chord for chord), `bev-system-chord`
  (Alt+4/Alt+7 resolve System/AppHistory), `bev-nav-vocabulary` (the nav tab set
  routes to `AppPage::ALL` in order onto shared icons), `bev-input-router`
  (page keys route through the shared router; wrong chords resolve to `None`).
- **P0-MC-03** (Applications page: process tree/table/search/identity/columns/
  icons): `bev-tree-projection`, `bev-tree-collapse`, `bev-table-wheel`,
  `bev-table-search-display`, `bev-input-arrows-seam`, `bev-input-search-owner`,
  `bev-input-semantic` (process-row semantic identity), `bev-control-disabled`,
  `bev-a11y-row-identity`, `bev-a11y-values`, `bev-sort-indicator`,
  `bev-icon-plates`.
- **P0-MC-06** (dangerous local confirmation-modal lifecycle):
  `bev-confirm-loop`, `bev-confirm-dismiss`, `bev-confirm-echo`,
  `bev-a11y-modal-dismiss`, `bev-input-delete-gate`, `bev-input-gate-priority`,
  `bev-input-escape-dismiss`.
- **P0-MC-07** (focus handling): `bev-input-ime` (Tab moves focus through the
  shared `MoveFocus` action; IME focus suppresses Delete/ArrowDown).

Exempt pairs (deliberate, with the reason):

- **P0-MC-01** (Battery/Fan Performance-page dynamic history): no declared Bevy
  case drives the Performance page's device history/readout. The `bev-chart-*`
  anchors drive the shared `widgets::chart` series widget, and `bev-history-*`
  drives the separate Application History page; neither is the Performance-page
  Battery/Fan history the requirement declares. Real undeclared Bevy tests exist
  (`pages::performance::tests::device_blocks_follow_the_projection_device_list`
  proves the dynamic device block list and per-device fact line, and
  `...::compact_device_activation_updates_local_state_and_shared_curve_focus`
  proves device activation/focus), so closing this pair is a
  **case-declaration** decision (add a Bevy row anchored to one of them), not a
  missing test.
- **P0-MC-02** (per-partition disk usage): no declared Bevy disk/partition/SMART
  interaction case.
- **P0-MC-04** (GPU selectable middle graph): no declared Bevy GPU/NPU
  interaction case.
- **P0-MC-05** (Preferences/sidebar/units/graph options): no declared Bevy
  settings/units/sidebar/density/hover interaction case.

Unmapped rows and why (26): the seven `bev-chart-*` / `bev-history-*` rows are
the shared-chart and Application-History surfaces (the P0-MC-01 near-misses
above); the twelve `bev-svc-menu-*`, `bev-startup-menu`, `bev-sessions-menu`,
`bev-menu-cancel` and `bev-log-*` rows drive service/startup/session control
menus and the service log panel, and no `P0-MC-*` requirement covers that
service-control surface; `bev-system-mount` / `bev-system-facts` project System
page content (not the navigation/page-set clause MC-00 declares);
`bev-window-drain` / `bev-input-quit-once` are app lifecycle; and
`bev-icon-stamp` / `bev-tofu-law` / `bev-bounded-lines` are render-time icon and
line mechanics rather than an interaction requirement (`bev-icon-plates`, the
icon-asset identity anchor the other frontends declare under MC-03, is mapped).

Evidence and verification (`cargo nextest list` discovery, never source
scanning): the resolver reports `bevy 4/8 (unmapped cells: 26)` with
`matrix 193 anchored / 0 dangling`, and a `--require-requirement-coverage` run is
red on exactly the four exempt pairs (`bevy: P0-MC-01/02/04/05`). No test id,
requirement id or path token was invented; every mapped row keeps its existing
`paths`, `contract_tag` and anchor.

Revisit condition: in the S5/S6 window, either declare new Bevy case rows
anchored to the real Performance/disk/GPU/settings tests above and add their
`p0_id`s (then the fail-closed flag can land), or keep the four pairs as the
standing exemption. Until one of those happens the requirement axis stays
`bevy 4/8` and the flag stays deferred.

Known S4/S5 residuals (owner decisions, not silently papered over):

- Bevy rows carry the D1 partial mapping: 26 rows declare a requirement and 26
  keep `-` (see D1 above), and every Bevy row still has no capture scenarios.
- **Resolved in W12-A (D5):** the nine Bevy path tokens (`confirmation`,
  `history`, `identity`, `layout`, `navigation`, `projection`, `render`,
  `route`, `selection`) are folded into the Rust `ContractTag` enum, and the
  enum's conformance test validates this matrix's `contract_tag` and `paths`
  columns. No vocabulary is copied into TSV, Python, or bash.
- **Resolved in W12-A (D2):** `mc02-tui-hotplug` is anchored to a real
  storage-family hot-unplug reconcile test and `mc07-tui-capture-visual` to the
  supervised capture frame's typed marker test.
- **Resolved by the W28 UI-parity line (D2):** the last two TUI `pending` cases
  were pointer-modality gaps the terminal shape does not port. Instead of
  accepting them, the parity line ported keyboard equivalents under their own
  case ids — `mc03-tui-column-reorder` (the column menu's `←`/`→` reorder) and
  `mc05-tui-chart-cursor` (the Performance·CPU chart's `←`/`→` sample cursor) —
  so the unified matrix is now 193 cases / 193 anchored / 0 pending / 0
  dangling. Both rows carry `success|keyboard`: the source case's `pointer` is
  not what a terminal keyboard port drives.
- TUI capture stays a single supervised frame (`scripts/capture-tui.sh`) with no
  scenario table, so TUI rows declare `capture_scenarios = -`; the anchored
  marker test proves the frame-marker contract, not a per-scenario matrix.
- **Iced capture now includes the health modal (W19-B).** Iced capture reaches
  the six shared pages, the eight Performance devices and two renderer-local
  surfaces - `service-details` on Services and the health modal on Performance.
  The scenario table's `device` token is resolved in the application by
  `capture_device_from_name` / `capture_page_from_name` plus those local-surface
  branches; the health branch opens through the same reducer the toolbar trigger
  dispatches (`Message::OpenHealth`), so no second opening path exists, and the
  first frame emits
  `ICED_CAPTURE_MARKER event=target_ready mode=demo page=performance device=health`
  (the modal rides the Performance page, so the validator's default page mapping
  applies unchanged). The scenario row is `health-modal` / `health` / `1180x780`
  and `mc06-local-modals` declares it as its capture scenario (the Iced
  compatibility matrix carries it from W19-B; the unified matrix row is synced
  in W22-B). The runner still carries no key or pointer injection: the surface
  is reached by a declared scenario token, never by synthesized input. Two
  evidence-frame properties are deliberate and must not be mistaken for
  product layout: the modal body stays the production fixed 430px scrollable,
  and a capture frame bounds it to its end (Iced clamps the requested offset
  to the real content height), so the thermal-zone panel - which sits below
  the device summary - is inside the frame; and the capture fixture publishes
  more than one thermal zone plus one unreadable zone, so the pixels show the
  per-reading traversal and the honest dash, never a fabricated `0.0 °C`. The
  headless anchor stays the traversal row
  in `feature_evidence.tsv`; the pixel frame is the targeted run
  `TM_ICED_CAPTURE_SCENARIOS=health-modal bash scripts/capture-iced.sh`,
  preconditioned on a Wayland session plus `dbus-run-session`,
  `kwin_wayland --virtual` and `niri` (the private supervisor route; the
  operator desktop is untouched and the run must not share the host with other
  heavy work). The application target, the scenario row and the matrix row land
  in one change, because the marker validator fails closed on any scenario name
  the application does not emit (`expected one target marker for 'health'`).
  The targeted run landed 2026-09-22 on a quiet host (clean worktree at
  `b7686a6b8a2a`): `TM_ICED_CAPTURE_SCENARIOS=health-modal bash
  scripts/capture-iced.sh` produced
  `target/iced-evidence/runs/0c738200-7a81-4ab7-95e0-67ec4a688bed/health-modal/image.png`
  (1368x888, 84052 B) with the matrix validator PASS and the recorded source
  manifest hash `c83fd5ce3f41973eb0593bb1f50d522e48ee0dd807e361c4b7caa226316f2dee`
  (recomputed identical from the worktree; the receipt satisfies the iced
  component of `ui-evidence-route.sh --require-capture`). The frame shows the
  `Thermal Zone Sensors` title, readable `Package` (51.0 °C) and `acpitz`
  (57.0 °C) rows, and the honest `nvme` dash, with the alert panel below them
  in frame, so the `power.thermal-zones` pixel cell is no longer a SKIP; the
  headless anchor above remains the always-run evidence.
- **TUI capture now covers the Fan page's system thermal group (W21-B).** The
  TUI route stays one supervised frame (`scripts/capture-tui.sh`, no scenario
  table); the Fan device reaches the system group through the capture fixture
  (`TM_TUI_CAPTURE_DEVICE=fan` seeds deterministic non-host readings:
  `hwmon:cpu` `cpu_fan` + `Package` 51.0 °C, readable `thermal:acpitz`
  61.0 °C, and permission-denied `thermal:nvme0`). The targeted run
  `TM_TUI_CAPTURE_PAGE=performance TM_TUI_CAPTURE_DEVICE=fan bash
  scripts/capture-tui.sh` landed 2026-09-22 (clean upstream tree at
  `16fea6372376`; the run recorded `worktree=dirty` only because an unrelated
  concurrent line held two non-TUI files modified) and produced
  `target/tui-evidence/runs/577ee263-933f-4be2-ac2d-e2d7ea62568d/tui-mvp.png`
  (1184x859, 66322 B, sha256
  `779f7b039021b9f79fa2a3af6f929f4a5f3e8f88b217e3b89533cfef4b7c8e80`) with
  `tui-capture-validation.json` `status=pass` and source manifest sha256
  `2343be520857edadaa69da5636ddc7c583c5735fcf933b46437e9cb3c11d1854`;
  `target/tui-evidence/latest` points at the run, and the receipt satisfies the
  TUI component of `ui-evidence-route.sh --require-capture` (diagnostic base
  `277f9b28~1`: the missing list named only `gpui-capture`, itself caused by
  the same diff touching `crates/taskmanager-ui-contract/*`). The frame shows
  the Fan selector `风扇 1 · · · · cpu_fan · 2400 RPM`, the device rows
  `转速 2400 RPM · PWM —` and `温度 Package · 51.0 °C`, then the system group
  header `温度` with `Package · 51.0 °C`, `acpitz · 61.0 °C` and the honest
  unread `nvme0 · —`, none of them clipped. Unit words are localized by the
  host locale (zh-CN: 转速 = speed, 温度 = temperature); source labels and
  numeric values are locale-independent, and the fixture never reads the host.
  The `power.thermal-zones` headless anchor in `feature_evidence.tsv` remains
  the always-run evidence; the pixel frame is the targeted run, preconditioned
  on a Wayland session plus `dbus-run-session`, `kwin_wayland --virtual` and
  `niri` and bounded by the supervisor's 30-minute limit.
- **UI evidence route impact table (W22-B).** `scripts/quality/ui-evidence-route.sh`
  keeps the headless requirement for every UI-boundary path but now demands
  fresh pixel receipts only from the frontends whose paint path a diff can
  move. `crates/taskmanager-ui-contract/` is no longer one GPUI glob:
  `tests/**`, `README.md`, the declaration registries (`capabilities.rs`,
  `conformance.rs`, `functional.rs`, `keybindings.rs`, `message.rs`,
  `feature_coverage*`) and the semantic accessibility model demand no pixel
  receipt (their evidence channel is the headless suite); `src/columns.rs`
  routes to GPUI + Iced + Bevy (the TUI owns its column model), `src/focus.rs`
  to GPUI, `src/navigation.rs` to GPUI + Iced + TUI, `src/icon.rs` and
  `src/command.rs` to all four, and every unlisted ui-contract path falls back
  to all four (fail-closed, never narrower than the old table). `locales/*`
  routes to all four because the catalog strings are `include_str!`-embedded
  by the shared application layer each product links, and
  `crates/taskmanager-icons/*` to GPUI + Iced + Bevy because the TUI maps
  `IconId` to terminal glyphs itself and does not link that crate. The
  requirement is derived from real render consumers (module imports under
  `crates/*/src`), never from a directory prefix. W21-B's diagnostic
  (`--base 277f9b28~1`, a TUI delivery plus a ui-contract gate-test change)
  reproduced the old over-demand (`missing: gpui-capture` only); under the new
  table the same diff is covered by the fresh TUI receipt and no GPUI receipt
  is owed. The conservative route direction is unchanged: it can demand more
  receipts than before (a shared path names every consumer), never fewer than
  its own paint impact.
- **Unified interaction route (W23-B).** The S5 wiring moved the headless
  interaction entry point to `scripts/parity/accept-frontend-interactions.sh`
  and the declaration to `scripts/parity/cross_frontend_matrix.tsv`. The route
  now consumes both: a change to the unified matrix maps to the frontends whose
  **rows changed** (read from the diff; a comment-only change moves no row and
  demands no receipt; an untracked/replaced file falls back to the frontends
  the file declares), which keeps per-frontend-matrix parity without a coarser
  "all four" guess, and the S5 driver, resolver, schema, declaration files and
  the aggregator are routed to the headless requirement only (dev-only evidence
  machinery, no paint path - the same class as the ui-contract
  declaration/test layers). The legacy per-frontend matrices and accept scripts
  stay routed while they remain compatibility assets, so the new wiring can
  never be weaker than the assets it is replacing.

### S5 status and remaining steps (W23-B)

Delivered: (S5-1) the unified driver is committed and the GPUI/Bevy interaction
stages of `local-gates.sh` run it; (S4) the unified matrix is the resolver's
declaration source; (W11-C) the gate consumes the unified matrix; (W23-B) the
feature co-anchor side table closes the cross-anchor record. Remaining, in
order, each requiring its own decision:

1. **D6 window**: retire the three per-frontend matrices and the embedded
   validators (`validate_gpui_interaction_matrix.py` structure rules, the
   Iced/Bevy in-script validators), migrate `scripts/windows/local-gates.sh`
   off the legacy gate, and delete the compatibility views in one cutover
   (AGENTS forbids a standing second address).
2. **D1 (resolved as partial mapping + exemption)**: the genuine Bevy rows are
   declared (bevy 4/8). The remaining work is the four exempt pairs: declare new
   cases anchored to the real Performance/disk/GPU/settings Bevy tests, or keep
   the exemption; `--require-requirement-coverage` can only be added to the
   `parity-evidence` stage once a fail-closed run is green.
3. **D3**: decide whether the GPUI stable case-prefix channel stays or the 39
   rows get hand-verified explicit `test_name` ids.
4. **Target ownership**: teach the resolver (or the driver) to carry the
   `gui`/`lib` target of each discovery artifact so a cross-target rename
   cannot pass on a flat discovery set.
5. **Co-anchor fold-in**: when a `crates/**` window can change the Rust
   consumer, fold `feature_evidence_co_anchors.tsv` into
   `feature_evidence.tsv` as a sixth column in one cutover (consumer, resolver,
   self-test, README in the same change) and delete the side table.

## Discipline

- Anchors are **declared by hand** and verified by set membership against
  `cargo nextest list`. Test ids are never derived by scanning Rust source; the
  resolver only reads the committed declaration files and the discovery
  artifact.
- No `file:line` evidence. No fabricated test ids.
- `test_name = pending` is an honest gap marker, not a pass: the row is counted
  and reported, never dangling, never covering a requirement. Promoting it to an
  anchor requires a hand-declared test id that discovery actually lists.
- `p0_id` references `scripts/interaction_requirements.tsv` (the public
  requirement vocabulary already used by the GPUI/Iced validators). The resolver
  never defines requirement ids and never writes the mapping; requirement
  coverage is a report (and, with `--require-requirement-coverage`, a
  fail-closed check), not a second vocabulary.
- `contract_tag` is a stable id of the Rust `ContractTag` enum
  (`crates/taskmanager-ui-contract/src/conformance.rs`), the single authority.
  The enum's conformance test reads this manifest and the unified matrix and
  rejects an unknown `contract_tag` or `paths` token; the resolver treats the
  field as opaque. Never copy the tag set into TSV, Python, or bash. The unified
  matrix's `contract_tag`/`paths` values reference that same vocabulary,
  including the Bevy tokens folded in by W12-A.
- `platform` is reserved and empty: the field is not allowed to become a second
  axis vocabulary in TSV/Python/bash. When P5 lands it gets its own single
  authority, and this resolver keeps treating it as opaque.
- `feature_id` in `feature_evidence.tsv` is the same kind of opaque reference:
  the vocabulary authority is the Rust `FeatureId::ALL` registry, and the
  contract test in `ui_feature_platform_gate.rs` parses the table and rejects an
  unknown id. The resolver never defines feature ids; it only resolves the
  anchored `test_id` values against discovery.
- A feature co-anchor (`feature_evidence_co_anchors.tsv`) is the same kind of
  hand-declared anchor as a primary one: it must name a discoverable test for
  the owning frontend, and it can never invent a clause, a status, or a
  vocabulary. The side table exists only because the Rust consumer of
  `feature_evidence.tsv` accepts exactly five columns; if the two ever
  disagree, the primary table is the authority.
- An interaction row carries **one** anchor, so its `paths` tokens must describe
  what that anchor really drives. A terminal shape whose click only selects (it
  never toggles, hovers or drags) must not claim `pointer` on a toggle or
  projection case: TUI's pointer path is anchored by its own case
  (`mc00-tui-nav-click`), and the toggle/aggregate rows carry `success`/
  `success|toggle` only. The same rule names the keyboard ports for what they
  are: `mc03-tui-column-reorder` and `mc05-tui-chart-cursor` carry
  `success|keyboard`, because a keyboard column reorder is not a pointer drag
  and a keyboard sample cursor is not a hover.
- Two interaction anchors record a deliberate evidence form rather than a
  missing one: `bev-tofu-law` anchors the spawned-icon scene (a bitmap
  `ImageNode` with no text glyph — the behavior the tofu law protects), while
  the source-codepoint scan stays in the suite as a static guard rather than an
  anchor; `iced mc03-icons` anchors the shared asset registry because iced's
  `icon()` returns an `Element` with no headless readback. Neither is a
  fabricated anchor, and both are recorded here so a reader does not mistake
  them for behavioral frame evidence.

## Local gate route

`scripts/quality/local-gates.sh standard` runs the `parity-evidence` stage,
which calls this resolver with `--nextest --scope auto`. Discovery compiles
test binaries, so the stage is diff-scoped like `ui-route`: it only evaluates
when the diff since `--base` (default: the merge-base with `origin/main`, else
`HEAD~1`) plus untracked files touches an evidence-relevant path:

```
scripts/parity/*                              # the manifest, unified matrix, resolver, self-test
crates/taskmanager-ui-contract/src/conformance.rs
crates/taskmanager-ui-contract/tests/*/ui_conformance.rs
crates/*/feature_coverage*                    # feature-coverage declarations + tests
```

Out of scope, the run reports `status: PASS (skipped)` and spawns no cargo. In
scope, `dangling` anchors fail closed and `pending` cells are only
counted/reported. Since W11-C the stage consumes **both** declaration sources in
one resolver pass: the facet manifest and the unified interaction matrix
(`--interaction-matrix scripts/parity/cross_frontend_matrix.tsv`), plus the
requirement vocabulary (`--requirements scripts/interaction_requirements.tsv`)
as a report-only coverage input. `--require-requirement-coverage` is the
deferred hard gate; its release condition is the four exempt Bevy pairs closing
(D1), at which point the stage adds the flag in the same change.

The feature-level evidence table is consumed by the same pass **without a
flag**: `--feature-evidence` defaults to
`scripts/parity/feature_evidence.tsv`, so the gate validates the feature
anchors (R4) exactly like the other two declaration sources, and its
`dangling` count feeds the same fail-closed exit. A commit that renames a test
referenced by the table is red at the `parity-evidence` stage. The sparse
feature co-anchor side table follows the same rule (`--co-anchors` defaults to
`scripts/parity/feature_evidence_co_anchors.tsv`), so a renamed co-anchor is
dangling in the same pass, and a missing side table is a stage error rather
than a silent skip.

### Interaction stages run the unified driver (S5 wiring, W23-B)

The `gpui-interactions` and `bevy-interactions` stages of
`scripts/quality/local-gates.sh` no longer call the legacy accept gate
directly. The committed call chain is:

```
local-gates.sh (--with-gui, or scope=bevy for the Bevy stage)
  -> bash scripts/parity/accept-frontend-interactions.sh <gpui|bevy> [--scope linux]
     -> python3 scripts/parity/accept_frontend_interactions.py
        1. scripts/frontend_source_manifest.py --frontend <fe>   # fingerprint
        2. bash scripts/accept-<fe>-interactions.sh              # legacy gate, UNCHANGED
        3. read the fresh target/<fe>-interaction-evidence/<run>/ receipts
        4. python3 scripts/parity/resolve_frontend_evidence.py   # Layer B, same discovery
        5. python3 scripts/quality/cross_frontend_manifest.py    # fold + --verify
```

Authority split, so the compatibility window is explicit:

- **The legacy per-frontend accept scripts stay the authoritative runners and
  receipt writers.** The unified driver owns the segment -> aggregate ->
  verify fold; it does not reimplement a per-frontend check, and it fails
  closed when the legacy gate exits non-zero, produces no fresh evidence
  directory, or fails to cover a declared cell.
- **The unified matrix is the declaration authority for the interaction
  resolver** (W11-C) and now also for the run-manifest aggregator. The
  per-frontend matrices (`scripts/{gpui,iced,bevy}_interaction_matrix.tsv`)
  remain committed compatibility views for the legacy validators until the
  D6/S5 retirement window; the unified driver's declaration is the unified
  matrix, so it cannot drift from the resolver's check.
- **The Windows mirror** (`scripts/windows/local-gates.sh`) still calls
  `scripts/windows/accept-gpui-interactions.sh` directly: it is outside the
  Linux line's boundary and migrates in the D6/S5 window, together with the
  legacy matrix/validator deletion.
- The stage keeps its external `timeout --kill-after=10s` deadline; the driver
  adds its own per-child deadlines and never backgrounds a child. No additional
  discovery pass is added for GPUI/ICED/Bevy (the driver reads the gate's own
  artifacts); only TUI, whose gate emits none, gets one bounded
  `cargo nextest list`.
- The Linux gate wires the GPUI and Bevy interaction stages - the two stages
  that existed before this change. Iced and TUI have no `local-gates`
  interaction stage, so nothing regressed: their interaction anchors are still
  discovery-checked by the `parity-evidence` stage through the unified matrix,
  and `bash scripts/parity/accept-frontend-interactions.sh iced|tui` runs the
  same driver chain when an owner opens those stages. A single-frontend driver
  run reports the resolver stage as skipped (the resolver resolves the whole
  declaration) rather than silently passing it.

Known boundary: an anchored test deleted or renamed *without* touching the
paths above stays invisible until one of them changes. That is the deliberate
cost trade; widening the trigger set (for example to every
`crates/taskmanager-{gpui,iced,tui,bevy-ui}/**` deletion) is a merge-owner
decision, not a silent default.

## Usage

```bash
# Contract-tag authority: validates every manifest `contract_tag` and every
# unified-matrix `contract_tag`/`paths` token against the Rust `ContractTag`
# enum (single source).  Also runs the ALL/from_id tests:
LANG=C cargo nextest run -p taskmanager-ui-contract -j 4

# Self-test (no cargo):
python3 scripts/parity/test_resolve_frontend_evidence.py

# Against pre-generated discovery (JSON from --message-format json, or --list text):
python3 scripts/parity/resolve_frontend_evidence.py \
  --discovery gpui=discovery/gpui.json \
  --discovery iced=discovery/iced.json \
  --discovery tui=discovery/tui.json \
  --discovery bevy=discovery/bevy.json

# Also resolve the unified interaction matrix (S4/W10-B; the gate now passes it):
python3 scripts/parity/resolve_frontend_evidence.py \
  --discovery gpui=discovery/gpui.json \
  --discovery iced=discovery/iced.json \
  --discovery tui=discovery/tui.json \
  --discovery bevy=discovery/bevy.json \
  --interaction-matrix scripts/parity/cross_frontend_matrix.tsv

# Add the requirement authority: per-frontend P0-MC coverage report and unknown
# p0_id rejection. With the D1 partial mapping Bevy is 4/8; a
# --require-requirement-coverage run is red on exactly the four declared-exempt
# (bevy, P0-MC-01/02/04/05) pairs, which is the expected honest result:
python3 scripts/parity/resolve_frontend_evidence.py \
  --discovery tui=discovery/tui.json --nextest \
  --interaction-matrix scripts/parity/cross_frontend_matrix.tsv \
  --requirements scripts/interaction_requirements.tsv

# Drive cargo directly (--locked, CARGO_BUILD_JOBS=4):
python3 scripts/parity/resolve_frontend_evidence.py --nextest

# Resolve the P5 feature-level evidence table explicitly (the default path is
# the same file, so the gate call above already consumes it):
python3 scripts/parity/resolve_frontend_evidence.py \
  --discovery gpui=discovery/gpui.json \
  --discovery iced=discovery/iced.json \
  --discovery tui=discovery/tui.json \
  --discovery bevy=discovery/bevy.json \
  --feature-evidence scripts/parity/feature_evidence.tsv

# The sparse feature co-anchor side table is default-on in the same pass
# (`--co-anchors`; a missing file is fail-closed), so the calls above already
# resolve every registered co-anchor:
python3 scripts/parity/resolve_frontend_evidence.py \
  --discovery gpui=discovery/gpui.json \
  --discovery iced=discovery/iced.json \
  --discovery tui=discovery/tui.json \
  --discovery bevy=discovery/bevy.json \
  --co-anchors scripts/parity/feature_evidence_co_anchors.tsv

# The unified S5 interaction driver (what the GPUI/Bevy interaction stages
# now run); a single frontend runs its legacy gate, folds one segment and
# verifies the aggregate:
bash scripts/parity/accept-frontend-interactions.sh gpui --scope linux
# No-cargo rehearsal from pre-existing discovery/native receipts:
bash scripts/parity/accept-frontend-interactions.sh iced --from-existing \
  --discovery iced=discovery/iced.json \
  --native-evidence iced=target/iced-interaction-evidence/<run> \
  --fingerprint current --source-manifest iced=target/frontend-source-manifests/iced.txt

# Local gate entry (the exact parity-evidence stage): skip when the diff cannot
# move an anchor, resolve both declaration sources, pin the report:
python3 scripts/parity/resolve_frontend_evidence.py --nextest --scope auto \
  --interaction-matrix scripts/parity/cross_frontend_matrix.tsv \
  --requirements scripts/interaction_requirements.tsv \
  --report-json target/cross-frontend-evidence/parity-evidence/manifest-validation.json
```

Exit codes: `0` every declared anchor resolved, or the diff was out of scope
(skipped); `1` dangling or invalid anchor (manifest, unified interaction matrix,
feature evidence, or feature co-anchor); `2` usage / IO / discovery error. A
skipped run is still
`status: "pass"` but carries `"skipped": true` and a `scope` block, so a
consumer can tell "verified" from "not applicable". The machine-readable report
lands at `target/cross-frontend-evidence/<run>/manifest-validation.json` by
default.
