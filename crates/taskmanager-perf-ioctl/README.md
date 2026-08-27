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
