# ADR-045: Iced cryoglyph uses the fixed `lru` line

## Status

Accepted for the `0.1.0` preparation line; remove the local patch when an
upstream-compatible `cryoglyph` release is available.

## Context

The Iced 0.14 GPU renderer resolves `cryoglyph` 0.1.0. That release declares
`lru` 0.16, while RustSec `RUSTSEC-2026-0253` affects `lru` releases below
0.18.2. The default GPUI release binary does not resolve this edge, but the
Iced product shape and its CI/visual matrix do. A lockfile-only update cannot
cross cryoglyph's `0.16` requirement.

## Decision

Vendor the exact `cryoglyph` 0.1.0 source under `patches/cryoglyph/` and change
only its dependency floor to `lru` 0.18.2. The root `[patch.crates-io]` entry
selects this source for every Iced build. No TaskForest business or renderer
API code is changed. `scripts/quality/dependency_floor_guard.py` is a hard
gate that rejects any `lru` lock entry below 0.18.2.

The RustSec ignore entry is removed. The patch is temporary: when upstream
cryoglyph publishes the same compatibility change, replace it with the
published release, rerun the Iced matrix, and delete the vendored source and
this ADR's temporary decision language in one focused change.

## Verification contract

- `cargo tree --workspace --all-features --target all` contains no `lru` below
  0.18.2;
- `cargo deny check advisories` has no lru exception;
- Iced workspace checks/tests and the full background Niri matrix pass;
- the GPUI, TUI, and Bevy shapes retain their existing gates;
- the patch contains the upstream license texts and no private evidence.
