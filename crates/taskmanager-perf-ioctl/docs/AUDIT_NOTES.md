# Audit notes

This crate is the audited `perf_event_open` boundary (ADR-022); the
unsafe/ABI containment rules are the crate boundary in the README.

`src/lib.rs` carries the `perf_event_open` ABI (`#[repr(C)]`) mapped to typed
per-engine usage, plus the pure constant ABI face (Miri gate, fuzz-exempt).

Permission denial, missing PMU, driver mismatch, counter reset and recovery
are normal typed outcomes.

## Verification

Run boundary unit tests, dependency firewall and Miri; real privileged PMU
success still requires a target-machine receipt:

```bash
cargo nextest run -p taskmanager-perf-ioctl -j 4
```
