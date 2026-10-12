# taskmanager-privilege-helper

## Role

Linux package for fixed-operation privileged helper binaries, invoked through
the escalation seam and emitting typed JSON envelopes.

## Boundary

Each binary target accepts one fixed operation, owns its privileged resource
for that invocation, and has a separate installed path and polkit action. The
package never becomes a general command runner.

## Contract and verification

The GPU PMU fixed-operation rule, typed outcome surface and receipt separation
are specified in [`docs/HELPER_CONTRACT.md`](docs/HELPER_CONTRACT.md). The
`taskforest-smart-self-test-helper` target also verifies a persistent disk
identity and the exact requested self-test capability before starting one test.

## Module map

```text
src/main.rs                  Intel GPU PMU helper target
src/smart_self_test_main.rs  SMART helper binary entry
src/smart_self_test.rs       identity-checked SMART test operation
src/discovery.rs  engine_names.rs   GPU engine discovery
src/sample.rs                PMU sampling
src/json.rs                  fixed JSON envelope output
```

## Docs

- [`docs/HELPER_CONTRACT.md`](docs/HELPER_CONTRACT.md) — the fixed-operation rule, contract
  surface and module notes.
- [ADR-022](../../adr/022-audited-perf-boundary-crate.md) — audited perf boundary.
