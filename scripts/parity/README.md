# Cross-frontend evidence manifest (P4 pilot)

This directory carries the P4 "evidence closure" pilot: a single committed
declaration manifest plus a resolver that checks every declared behavior anchor
against real test discovery.

- `cross_frontend_manifest.tsv` — the declaration list. One row per
  `(subject_kind, subject_id, frontend)`.
- `resolve_frontend_evidence.py` — Layer B resolver. It compares declared
  `behavior` anchors with `cargo nextest list` output and fails on dangling
  anchors (R4) or target/frontend mismatches (R6). It owns no contract-tag
  vocabulary: `contract_tag` is an opaque required field here. `--scope auto`
  is the local-gate entry: it short-circuits when the git diff since `--base`
  cannot move an anchor (see "Local gate route"), and `--report-json` pins the
  machine-readable report path.
- `test_resolve_frontend_evidence.py` — standard-library self-test proving the
  resolver rejects a deleted referenced test, a target mismatch, a duplicate
  cell, malformed declarations, and that the diff-scope only skips when no
  evidence-relevant path changed (fail-closed on a failed diff probe).

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

## Discipline

- Anchors are **declared by hand** and verified by set membership against
  `cargo nextest list`. Test ids are never derived by scanning Rust source; the
  resolver only reads the manifest and the discovery artifact.
- No `file:line` evidence. No fabricated test ids.
- `contract_tag` is a stable id of the Rust `ContractTag` enum
  (`crates/taskmanager-ui-contract/src/conformance.rs`), the single authority.
  The enum's conformance test reads this manifest and rejects an unknown tag;
  the resolver treats the field as opaque. Never copy the tag set into TSV,
  Python, or bash.
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
scripts/parity/*                              # the manifest, resolver, self-test
crates/taskmanager-ui-contract/src/conformance.rs
crates/taskmanager-ui-contract/tests/*/ui_conformance.rs
crates/*/feature_coverage*                    # feature-coverage declarations + tests
```

Out of scope, the run reports `status: PASS (skipped)` and spawns no cargo. In
scope, `dangling` anchors fail closed and `pending` cells are only
counted/reported.

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

# Drive cargo directly (--locked, CARGO_BUILD_JOBS=4):
python3 scripts/parity/resolve_frontend_evidence.py --nextest

# Local gate entry: skip when the diff cannot move an anchor, pin the report:
python3 scripts/parity/resolve_frontend_evidence.py --nextest --scope auto \
  --report-json target/cross-frontend-evidence/parity-evidence/manifest-validation.json
```

Exit codes: `0` every declared anchor resolved, or the diff was out of scope
(skipped); `1` dangling or invalid anchor; `2` usage / IO / discovery error. A
skipped run is still `status: "pass"` but carries `"skipped": true` and a
`scope` block, so a consumer can tell "verified" from "not applicable". The
machine-readable report lands at
`target/cross-frontend-evidence/<run>/manifest-validation.json` by default.
