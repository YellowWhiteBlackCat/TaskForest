# Cross-frontend evidence manifest (P4 pilot)

This directory carries the P4 "evidence closure" pilot: a single committed
declaration manifest plus a resolver that checks every declared behavior anchor
against real test discovery.

- `cross_frontend_manifest.tsv` — the declaration list. One row per
  `(subject_kind, subject_id, frontend)`.
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
  `pending` rows. It owns no contract-tag vocabulary (`contract_tag` is an
  opaque required field) and no requirement vocabulary (`p0_id` is opaque
  unless `--requirements` supplies the public id list). `--scope auto` is the
  local-gate entry: it short-circuits when the git diff since `--base` cannot
  move an anchor (see "Local gate route"), and `--report-json` pins the
  machine-readable report path.
- `test_resolve_frontend_evidence.py` — standard-library self-test proving the
  resolver rejects a deleted referenced test, a target mismatch, a duplicate
  cell, malformed declarations, malformed unified-matrix rows and a deleted
  interaction anchor on either channel, counts `pending` rows instead of
  dangling them, validates requirement coverage fail-closed, and only skips the
  diff-scope when no evidence-relevant path changed (fail-closed on a failed
  diff probe).

The `contract_tag` vocabulary itself is the Rust
[`ContractTag`](../../crates/taskmanager-ui-contract/src/conformance.rs) enum.
Its conformance test reads `cross_frontend_manifest.tsv` and fails on any
unknown tag, so the tag set has exactly one authority (Rust) and the manifest
only references its ids.

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

The TUI block (47 rows: 43 anchored + 4 `pending`) is the D2 deliverable. TUI
has no stable case-prefix convention and no per-frontend matrix, so every row
names its test explicitly; the `pending` rows record honest gaps that could not
be anchored to a discoverable test (`mc02-tui-hotplug`, `mc03-tui-column-drag`,
`mc05-tui-chart-hover`, `mc07-tui-capture-visual`). All 47 rows declare a
`P0-MC-*` id; together with GPUI/Iced they cover all eight requirements on
three frontends. Bevy still carries `-` (see "Bevy `p0_id`" below).

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

Consumption is opt-in: `scripts/quality/local-gates.sh` still calls the resolver
without `--interaction-matrix`, so the unified matrix changes no gate until the
S5 driver wiring lands (see the private convergence plan). The old matrices stay
authoritative for the accept scripts until then.

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
- Nine Bevy path tokens (`confirmation`, `history`, `identity`, `layout`,
  `navigation`, `projection`, `render`, `route`, `selection`) are not folded
  into the Rust `ContractTag` enum yet; the enum currently folds only the
  GPUI/Iced path set. The unified matrix exposes this as an S4/S5 divergence
  instead of copying a fourth vocabulary; extending the authority is a
  `crates/**` change outside this wave.
- TUI capture is a single supervised frame (`scripts/capture-tui.sh`) with no
  scenario table, so TUI rows declare `capture_scenarios = -` and the
  capture-readiness case is `pending`.

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
  The enum's conformance test reads this manifest and rejects an unknown tag;
  the resolver treats the field as opaque. Never copy the tag set into TSV,
  Python, or bash. The unified matrix's `contract_tag`/`paths` values reference
  that same vocabulary; the Bevy tokens that are not folded into the enum yet
  are listed as an S4 residual above and must be folded in Rust, never here.
- `platform` is reserved and empty: the field is not allowed to become a second
  axis vocabulary in TSV/Python/bash. When P5 lands it gets its own single
  authority, and this resolver keeps treating it as opaque.

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
counted/reported. The stage currently consumes only the facet manifest; passing
`--interaction-matrix` is the S5 driver wiring step and is deliberately not
enabled here.

Known boundary: an anchored test deleted or renamed *without* touching the
paths above stays invisible until one of them changes. That is the deliberate
cost trade; widening the trigger set (for example to every
`crates/taskmanager-{gpui,iced,tui,bevy-ui}/**` deletion) is a merge-owner
decision, not a silent default.

## Usage

```bash
# Contract-tag authority: validates every manifest contract_tag against the
# Rust `ContractTag` enum (single source).  Also runs the ALL/from_id tests:
LANG=C cargo nextest run -p taskmanager-ui-contract -j 4

# Self-test (no cargo):
python3 scripts/parity/test_resolve_frontend_evidence.py

# Against pre-generated discovery (JSON from --message-format json, or --list text):
python3 scripts/parity/resolve_frontend_evidence.py \
  --discovery gpui=discovery/gpui.json \
  --discovery iced=discovery/iced.json \
  --discovery tui=discovery/tui.json \
  --discovery bevy=discovery/bevy.json

# Also resolve the unified interaction matrix (S4/W10-B; opt-in):
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

# Local gate entry: skip when the diff cannot move an anchor, pin the report:
python3 scripts/parity/resolve_frontend_evidence.py --nextest --scope auto \
  --report-json target/cross-frontend-evidence/parity-evidence/manifest-validation.json
```

Exit codes: `0` every declared anchor resolved, or the diff was out of scope
(skipped); `1` dangling or invalid anchor (manifest or unified interaction
matrix); `2` usage / IO / discovery error. A skipped run is still
`status: "pass"` but carries `"skipped": true` and a `scope` block, so a
consumer can tell "verified" from "not applicable". The machine-readable report
lands at `target/cross-frontend-evidence/<run>/manifest-validation.json` by
default.
