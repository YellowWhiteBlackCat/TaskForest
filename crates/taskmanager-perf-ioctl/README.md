# taskmanager-perf-ioctl

## Role

Audited `perf_event_open` boundary for constrained Intel GPU/PMU telemetry
(ADR-022).

## Boundary

The crate owns raw syscall arguments, ABI size checks, fd lifetime, bounded
reads and kernel error mapping. Its public API exposes typed samples/errors;
no raw pointer, handle or platform policy crosses the boundary.

## Contract and verification

Permission denial, missing PMU, driver mismatch, counter reset and recovery are
normal typed outcomes. Run boundary unit tests, dependency firewall and Miri;
real privileged PMU success still requires a target-machine receipt.

## Module map

```text
src/lib.rs    perf_event_open ABI (#[repr(C)]) → typed per-engine usage;
              pure constant ABI face (Miri gate, fuzz-exempt)
```

## Docs

- [`docs/AUDIT_NOTES.md`](docs/AUDIT_NOTES.md) — boundary ownership, typed outcomes and
  verification.
- [ADR-022](../../adr/022-audited-perf-boundary-crate.md) — audited perf boundary.
