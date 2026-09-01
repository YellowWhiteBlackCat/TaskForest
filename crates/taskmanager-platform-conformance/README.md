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

Keep assertions portable across minimal runners. Each adapter runs the same
contract on its native host; the suite proves shared semantics, not hardware
coverage or pixel acceptance.

## Module map

```text
src/capability.rs  identity.rs  process.rs    host-agnostic assertions
src/smoke.rs  source.rs        live-drain ownership and composition-edge scenarios
```

Run against real hosts by each adapter's tests/conformance.rs and the root live smoke.
