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
  `pending` rows, and with `--feature-evidence` (default:
  `scripts/parity/feature_evidence.tsv`, so it is on in the gate) the P5
  feature-level table as a third. It owns no contract-tag vocabulary
  (`contract_tag` is an opaque required field), no requirement vocabulary
  (`p0_id` is opaque unless `--requirements` supplies the public id list), and
  no feature vocabulary (`feature_id` is opaque; the Rust registry owns it).
  `--scope auto` is the local-gate entry: it short-circuits when the git diff
  since `--base` cannot move an anchor (see "Local gate route"), and
  `--report-json` pins the machine-readable report path.
- `test_resolve_frontend_evidence.py` — standard-library self-test proving the
  resolver rejects a deleted referenced test, a target mismatch, a duplicate
  cell, malformed declarations, malformed unified-matrix rows, a deleted
  interaction anchor on either channel, and a deleted feature-level anchor;
  counts `pending` rows instead of dangling them, validates requirement
  coverage fail-closed, and only skips the diff-scope when no
  evidence-relevant path changed (fail-closed on a failed diff probe).

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
  `power.thermal-throttle-events`, which this shape declares `Unsupported`, and
  gained no row.
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

The table now carries **58 anchored + 8 `pending`** rows (per frontend: gpui 9,
iced 17, tui 17, bevy 15 anchored; pending: gpui 6, tui 2, bevy 0). No `pending`
row remains on a shape whose delivered surface a real test proves.
Every other source-complete cell keeps its G2 finding until a real test is
anchored; the batches are a bounded delivery, never a blanket `Ready` claim. A
`pending` row is a survey record, not a delivery claim, and it never becomes an
anchor.

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
| `p0_id` | public requirement id from `scripts/interaction_requirements.tsv`; `-` where no mapping is declared yet (all Bevy rows) |
| `target` | `gui` or `lib` (matrix semantics, unchanged) |
| `test_name` | nextest test path; `-` when the row uses a stable case-prefix channel; `pending` when the case is declared but no discoverable anchor exists yet |
| `paths` | ordered behavior/contract paths (matrix semantics, unchanged) |
| `capture_scenarios` | `-` or `|`-joined capture scenario names (matrix semantics, unchanged) |
| `contract_tag` | the row's primary contract tag: it must equal the first `paths` token. Opaque to the resolver, whose only check is that matrix-internal equality; the vocabulary authority stays the Rust `ContractTag` enum. |
| `platform` | **reserved P5 axis**, same discipline as the manifest: empty in every committed row and opaque to the resolver. |

The TUI block (47 rows: 45 anchored + 2 `pending`) is the D2 deliverable. TUI
has no stable case-prefix convention and no per-frontend matrix, so every row
names its test explicitly; the `pending` rows record honest gaps that could not
be anchored to a discoverable test (`mc03-tui-column-drag`,
`mc05-tui-chart-hover`). Both are pointer-modality cases the terminal shape does
not port: the TUI capability registry declares `ColumnDragResize` and `Tooltip`
unsupported ("no pointer-driven column-edge drag surface", "no hover surface"),
and the runtime deliberately drops drag/move events as unmodeled. The other two
D2 gaps were closed by real tests, not relabelled: `mc02-tui-hotplug` anchors
the storage-family fail-closed fallback and `mc07-tui-capture-visual` anchors
the supervised capture frame's typed marker (see "Known S4/S5 residuals"). All
47 rows declare a `P0-MC-*` id; together with GPUI/Iced they cover all eight
requirements on three frontends. Bevy still carries `-` (see "Bevy `p0_id`"
below).

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
existing callers are unaffected. This mechanism is what the two Bevy `p0_id`
options below would use; it does not decide the mapping.

The `parity-evidence` gate stage passes `--requirements` and deliberately omits
`--require-requirement-coverage`. So the stage records the per-frontend coverage
report and rejects an unknown `p0_id`, but an uncovered `(frontend, requirement)`
pair does not fail the run while Bevy is 0/8. Release condition: when the Bevy
`p0_id` mapping lands (D1 option A), the same change that fills the mapping adds
`--require-requirement-coverage` to the stage and the requirement axis becomes a
fail-closed 8/8 x 4 check.

### Bevy `p0_id` (D1 open decision)

No authority in the repository maps Bevy interaction cases to `P0-MC-*`: the
Bevy compatibility matrix has no `p0_id` column, and in committed files `P0-MC`
appears only in `scripts/interaction_requirements.tsv`, the GPUI/Iced matrices,
this unified matrix, and the private design note. W9-C/W10-B refuse to invent
the mapping, so all 52 Bevy rows stay `-`. Two executable options exist:

- **Option A (recommended): declare the mapping by hand, per Bevy row.** The
  owner (or a designated Bevy line) fills the 52 `p0_id` cells from the same
  requirement semantics GPUI/Iced declare, then the run is validated with
  `--require-requirement-coverage`: every anchor stays discovery-checked and the
  coverage block proves 8/8 × 4 mechanically. Cost: one declaration pass plus
  review; risk: mapping disputes, mitigated by the coverage report naming the
  uncovered pairs. No code or schema change is needed — the mechanism is
  already here, and this is the only option that closes the four-frontend
  requirement claim.
- **Option B: formalize the exemption.** Keep `-` for Bevy and record the
  "no mapping authority" status in the private decision register; the
  four-frontend claim is then scoped to cases (not requirements) for Bevy. Cost:
  zero now; risk: the requirement axis stays 3/4 indefinitely and will fail any
  future `--require-requirement-coverage` gate on Bevy.

Owner action (D1): pick A or B. If A, designate the reviewer and the review
turn; if B, record the exemption expiry so it is revisited with S5/S6.

Known S4/S5 residuals (owner decisions, not silently papered over):

- Bevy rows have no `p0_id` mapping (kept `-`; see D1 above) and no capture
  scenarios.
- **Resolved in W12-A (D5):** the nine Bevy path tokens (`confirmation`,
  `history`, `identity`, `layout`, `navigation`, `projection`, `render`,
  `route`, `selection`) are folded into the Rust `ContractTag` enum, and the
  enum's conformance test validates this matrix's `contract_tag` and `paths`
  columns. No vocabulary is copied into TSV, Python, or bash.
- **Resolved in W12-A (D2):** `mc02-tui-hotplug` is anchored to a real
  storage-family hot-unplug reconcile test and `mc07-tui-capture-visual` to the
  supervised capture frame's typed marker test. The two remaining TUI `pending`
  cases are the pointer-modality gaps the terminal shape does not port
  (`mc03-tui-column-drag`, `mc05-tui-chart-hover`); the capability registry owns
  the absence reasons and neither case can be promoted without a real ported
  surface. Owner decision (D2): accept the two gaps, or open a UI-parity line
  that ports a keyboard equivalent and gets its own case id (a keyboard sample
  cursor is not "hover").
- TUI capture stays a single supervised frame (`scripts/capture-tui.sh`) with no
  scenario table, so TUI rows declare `capture_scenarios = -`; the anchored
  marker test proves the frame-marker contract, not a per-scenario matrix.

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
deferred hard gate; its release condition is the Bevy `p0_id` mapping landing
(D1), at which point the stage adds the flag in the same change.

The feature-level evidence table is consumed by the same pass **without a
flag**: `--feature-evidence` defaults to
`scripts/parity/feature_evidence.tsv`, so the gate validates the feature
anchors (R4) exactly like the other two declaration sources, and its
`dangling` count feeds the same fail-closed exit. A commit that renames a test
referenced by the table is red at the `parity-evidence` stage.

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

# Add the requirement authority: per-frontend P0-MC coverage report, unknown
# p0_id rejection, and fail-closed coverage (Bevy has no mapping yet, so
# --require-requirement-coverage is red on Bevy until D1 is decided):
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

# Local gate entry (the exact parity-evidence stage): skip when the diff cannot
# move an anchor, resolve both declaration sources, pin the report:
python3 scripts/parity/resolve_frontend_evidence.py --nextest --scope auto \
  --interaction-matrix scripts/parity/cross_frontend_matrix.tsv \
  --requirements scripts/interaction_requirements.tsv \
  --report-json target/cross-frontend-evidence/parity-evidence/manifest-validation.json
```

Exit codes: `0` every declared anchor resolved, or the diff was out of scope
(skipped); `1` dangling or invalid anchor (manifest or unified interaction
matrix); `2` usage / IO / discovery error. A skipped run is still
`status: "pass"` but carries `"skipped": true` and a `scope` block, so a
consumer can tell "verified" from "not applicable". The machine-readable report
lands at `target/cross-frontend-evidence/<run>/manifest-validation.json` by
default.
