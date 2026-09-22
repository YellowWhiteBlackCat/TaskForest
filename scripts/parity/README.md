# Cross-frontend evidence manifest (P4 pilot)

This directory carries the P4 "evidence closure" pilot: a single committed
declaration manifest plus a resolver that checks every declared behavior anchor
against real test discovery.

- `cross_frontend_manifest.tsv` — the declaration list. One row per
  `(subject_kind, subject_id, frontend)`.
- `cross_frontend_matrix.tsv` — the unified interaction matrix (S4). It carries
  the three per-frontend interaction matrices as one list with a `frontend`
  dimension. The original `scripts/{gpui,iced,bevy}_interaction_matrix.tsv`
  stay committed as **compatibility views** (read by the existing accept
  scripts and validators) until the S5 retirement wave moves those callers.
- `resolve_frontend_evidence.py` — Layer B resolver. It compares declared
  `behavior` anchors with `cargo nextest list` output and fails on dangling
  anchors (R4) or target/frontend mismatches (R6). With `--interaction-matrix`
  it consumes the unified matrix as a second declaration source. It owns no
  contract-tag vocabulary: `contract_tag` is an opaque required field here.
  `--scope auto` is the local-gate entry: it short-circuits when the git diff
  since `--base` cannot move an anchor (see "Local gate route"), and
  `--report-json` pins the machine-readable report path.
- `test_resolve_frontend_evidence.py` — standard-library self-test proving the
  resolver rejects a deleted referenced test, a target mismatch, a duplicate
  cell, malformed declarations, malformed unified-matrix rows and a deleted
  interaction anchor on either channel, and that the diff-scope only skips when
  no evidence-relevant path changed (fail-closed on a failed diff probe).

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

`cross_frontend_matrix.tsv` is the S4 unified list: one row per
`(frontend, case_id)`, mechanically transcribed from the three per-frontend
matrices, with anchors declared by hand and verified by set membership against
`cargo nextest list`. Nothing was derived by scanning Rust source.

| column | meaning |
|---|---|
| `subject_kind` | `interaction` (the new subject type; the schema reserves room for future `requirement` rows) |
| `case_id` | stable case id from the owning frontend's matrix |
| `frontend` | `gpui`, `iced`, `tui`, or `bevy` — the new dimension |
| `p0_id` | public requirement id from `scripts/interaction_requirements.tsv`; `-` where no mapping is declared yet (all Bevy rows) |
| `target` | `gui` or `lib` (matrix semantics, unchanged) |
| `test_name` | nextest test path; `-` when the row uses a stable case-prefix channel |
| `paths` | ordered behavior/contract paths (matrix semantics, unchanged) |
| `capture_scenarios` | `-` or `|`-joined capture scenario names (matrix semantics, unchanged) |
| `contract_tag` | the row's primary contract tag: it must equal the first `paths` token. Opaque to the resolver, whose only check is that matrix-internal equality; the vocabulary authority stays the Rust `ContractTag` enum. |
| `platform` | **reserved P5 axis**, same discipline as the manifest: empty in every committed row and opaque to the resolver. |

Anchor-source recognition in the resolver (`--interaction-matrix PATH`):

- rows with an explicit `test_name` resolve by exact membership in the owning
  frontend's discovery (iced/bevy rows);
- rows with `test_name = -` use the owning frontend's **stable case-prefix
  channel** — accepted only for frontends listed in `CASE_PREFIX_CHANNELS`
  (GPUI: `<case_id>` with `-` normalized to `_`, plus `_case_`), mirroring the
  existing GPUI validator;
- either way a missing anchor is `dangling` (R4) and fails the run.

Consumption is opt-in: `scripts/quality/local-gates.sh` still calls the resolver
without `--interaction-matrix`, so the unified matrix changes no gate until the
S5 driver wiring lands (see the private convergence plan). The old matrices stay
authoritative for the accept scripts until then.

Known S4 residuals (owner decisions, not silently papered over):

- `tui` has no interaction matrix yet; the unified list only carries gpui,
  iced, and bevy rows.
- Bevy rows have no `p0_id` mapping (kept `-`) and no capture scenarios.
- Nine Bevy path tokens (`confirmation`, `history`, `identity`, `layout`,
  `navigation`, `projection`, `render`, `route`, `selection`) are not folded
  into the Rust `ContractTag` enum yet; the enum currently folds only the
  GPUI/Iced path set. The unified matrix exposes this as an S4/S5 divergence
  instead of copying a fourth vocabulary; extending the authority is a
  `crates/**` change outside this wave.

## Discipline

- Anchors are **declared by hand** and verified by set membership against
  `cargo nextest list`. Test ids are never derived by scanning Rust source; the
  resolver only reads the committed declaration files and the discovery
  artifact.
- No `file:line` evidence. No fabricated test ids.
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

# Also resolve the unified interaction matrix (S4; opt-in):
python3 scripts/parity/resolve_frontend_evidence.py \
  --discovery gpui=discovery/gpui.json \
  --discovery iced=discovery/iced.json \
  --discovery tui=discovery/tui.json \
  --discovery bevy=discovery/bevy.json \
  --interaction-matrix scripts/parity/cross_frontend_matrix.tsv

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
