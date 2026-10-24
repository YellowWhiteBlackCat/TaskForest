# taskmanager-daemon

Background telemetry collector, persistence owner, and IPC daemon service for TaskForest.

## Role

Runs as an independent, lightweight system service (systemd on Linux, Windows Service on Windows).
Owns 7×24 continuous background metric sampling and persistent rolling history under ADR-055,
and serves unprivileged frontend subscriber clients via `taskmanager-ipc`.

## Boundary

- **Strict Safe Rust**: The entire crate is strictly `#![forbid(unsafe_code)]`.
- **Single-Writer Lock**: Exclusively holds the unique `taskmanager-history-store` writer lock when running. Frontends in daemon mode never write directly to disk.
- **On-Demand Throttling**: Automatically downscales sampling cadence and skips high-overhead facets when no client subscribes.
- **Unprivileged Service**: Operates with minimal necessary permissions, mediating privileged actions via existing audited helpers under ADR-023.

## Contract and verification

The daemon implements ADR-055's dual-mode background tier.
Verification covers socket lifecycle, connection handling, multi-client subscription aggregation,
and graceful shutdown without resource leakage or lock orphaning.

## Module map

```text
src/
├── collector.rs      // Background metric collection loops and platform collector abstraction
├── config.rs         // Daemon runtime configuration and defaults
├── state.rs          // Active subscription state and cadence arbitrator
├── persistence.rs    // Background history persistence integration
├── service.rs        // Main service loop, socket server and shutdown coordinator
└── lib.rs            // Public library entrypoint and exports
```
