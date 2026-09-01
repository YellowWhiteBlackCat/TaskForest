# taskmanager-rapl-helper

## Role

One-shot Linux helper for Intel RAPL package power, invoked through the
escalation seam and emitting a typed JSON envelope.

## Boundary

The helper performs exactly one operation: sample every top-level
`/sys/class/powercap/intel-rapl:N` package's `energy_uj` twice (1000 ms
apart) and reduce the delta — with wraparound handling against
`max_energy_range_uj` — to per-package watts. No flags, no other path, no
general command running.

## Contract and verification

Exit codes (0 success; 2 permission_denied, 3 no_rapl, 4 open_failed, 5
read_failed), the field-disjoint SUCCESS/ERROR envelopes, wraparound and
unknowable-delta skips, package sorting, top-level-only selection, and denial
classification are covered by headless fixture tests (the wait is injected, so
the two-read contract stays deterministic):

```bash
cargo nextest run -p taskmanager-rapl-helper -j 4 --all-targets
cargo clippy -p taskmanager-rapl-helper --all-targets -- -D warnings
```

Package/polkit verification and a live RAPL receipt are separate; without the
latter the capability remains permission-limited.

## Module map

```text
src/main.rs → rapl_read.rs   read-only RAPL package-power sample (energy_uj)
src/json.rs                  fixed JSON envelope output
```
