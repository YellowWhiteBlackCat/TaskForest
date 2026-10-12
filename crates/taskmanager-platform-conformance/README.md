# taskmanager-platform-conformance

## Role

Host-neutral conformance scenarios shared by Linux, macOS and Windows adapter
tests and the live smoke gate.

## Boundary

Scenarios assert capabilities, process-row invariants, live drain ownership,
typed failure semantics, wrong-identity zero-side-effect rejection and device
discovery coherence. They contain no OS I/O, target-specific paths or
`cfg(target_os)` provider logic.

## Contract and verification

Scenario contracts stay portable across minimal runners and prove shared
semantics, not hardware coverage or pixel acceptance. The publication-liveness
budget, the capability-surface same-source scenario and the
escalation/permission capability invariant are specified in
[`docs/SCENARIOS.md`](docs/SCENARIOS.md).

## Module map

```text
src/capability.rs  escalation.rs  identity.rs   host-agnostic assertions
src/process.rs     source.rs      smoke.rs      row/live-drain scenarios
src/smoke_budget.rs                            native publication wait budget
```

Run against real hosts by each adapter's tests/conformance.rs and the root live smoke.

## Docs

- [`docs/SCENARIOS.md`](docs/SCENARIOS.md) — the capability-surface same-source scenario and
  the escalation/permission capability invariant.
- [ADR-053](../../adr/053-product-expected-capability-surface.md) — product-expected
  capability surface.
