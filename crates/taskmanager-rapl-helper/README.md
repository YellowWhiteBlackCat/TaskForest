# taskmanager-rapl-helper

## Role

One-shot Linux helper for Intel RAPL package power, invoked through the
escalation seam and emitting a typed JSON envelope.

## Boundary

The helper performs exactly one operation: a two-read RAPL package-power
sample reduced to per-package watts with wraparound handling (register-level
detail: [`docs/SAMPLING_CONTRACT.md`](docs/SAMPLING_CONTRACT.md)). No flags,
no other path, no general command running.

## Contract and verification

The two-read delta contract, wraparound handling, exit codes and the fixture
verification route are specified in
[`docs/SAMPLING_CONTRACT.md`](docs/SAMPLING_CONTRACT.md).

## Module map

```text
src/main.rs → rapl_read.rs   read-only RAPL package-power sample (energy_uj)
src/json.rs                  fixed JSON envelope output
```

## Docs

- [`docs/SAMPLING_CONTRACT.md`](docs/SAMPLING_CONTRACT.md) — the two-read delta contract,
  wraparound handling and verification.
- [ADR-023](../../adr/023-per-feature-privilege-escalation-framework.md) — escalation
  framework.
