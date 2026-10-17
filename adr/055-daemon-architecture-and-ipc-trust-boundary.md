# ADR-055: Daemon architecture, IPC trust boundary, and persistence session lifecycle

Status: accepted (extends ADR-036 rule 6; adheres to ADR-005 inward dependency and ADR-023 unprivileged application model)

## Context

ADR-036 scoped history recording to the active frontend process lifetime to eliminate
uncontrollable background collector processes. Rule 6 of ADR-036 explicitly stated:
*"Recording while every frontend is closed is intentionally not provided by this ADR.
A future cross-frontend session owner or IPC design requires a separate trust-boundary
ADR and must not reintroduce an implicit collector."*

As TaskForest advances to v0.3.0+, long-term continuous system monitoring (7×24 background
telemetry, soak stability, historical trend analysis) and privilege separation require
a dedicated system daemon. Running collection entirely in GUI processes forces elevated
privileges into desktop UI toolkits and drops all metrics the moment a window is closed.

## Decision

1. **Dual-Mode Engine (Daemon Mode with Standalone Fallback)**:
   - Frontends (`taskforest-g`, `taskforest-i`, `taskmanager-tui`, `taskforest-b`) consume
     a unified `AppHostMode` connector.
   - On startup, the frontend probes the local platform IPC channel (Unix Domain Socket on
     Linux/macOS, Named Pipe on Windows).
   - If the daemon service is active and responsive, the frontend connects as an unprivileged
     subscriber client, pulling continuous historical series and streaming real-time metrics.
   - If the daemon is absent, inactive, or uninstalled, the frontend falls back seamlessly
     to the standalone in-process direct sampling path defined in ADR-036. Standalone portability
     is never broken.

2. **Single-Writer Lock Ownership and Session Lifecycle**:
   - The unique persistence writer lock (`taskmanager-history-store`) is owned exclusively
     by the daemon when running as a service. Frontends in daemon mode never attempt filesystem
     persistence writes.
   - In standalone fallback mode, the first active frontend acquires the session writer lock
     under ADR-036 semantics.
   - Multiple frontends can simultaneously connect to the daemon over IPC and subscribe to
     telemetry without lock contention.

3. **Privilege Separation and Attack Surface Reduction**:
   - The daemon process handles privilege escalation helpers (e.g. Polkit process-control,
     MSR, SMART self-test) under ADR-023 boundaries.
   - GUI frontends run strictly unprivileged, eliminating direct root/admin exposure from
     complex UI rendering stacks (Wayland, OpenGL, Vulkan, DirectWrite).

4. **On-Demand Facet Sampling and Sleep Throttling**:
   - The daemon tracks an aggregated `SubscriptionMask` across all connected clients.
   - High-overhead facets (e.g. per-socket network attribution, deep process tree scans)
     are sampled only when at least one connected client requests them.
   - When zero clients are connected, the daemon drops into background sleep mode (e.g. 10s
     cadence for fundamental CPU/RAM/Disk health), keeping background resource consumption < 0.2% CPU.

5. **Transport and Framing Contract**:
   - IPC communication is owned by `taskmanager-ipc`.
   - Every message uses 20-byte framing (`TFIP` magic, monotonic sequence ID, CRC32 verification,
     and strict 16 MiB payload ceilings) to prevent buffer corruption or memory exhaustion.

## Consequences

- 7×24 continuous history is achieved without making the GUI permanent or untrusted.
- Closing a frontend window no longer loses telemetry history. Opening a frontend immediately
  visualizes unbroken historical charts.
- Standalone portable execution remains 100% functional for containers, live USBs, and
  non-root environments.
- Packaging can distribute `taskforest-daemon` as an optional background systemd unit/service.

## Verification

- Daemon connection, subscription aggregation, and seamless standalone fallback are verified
  via headless behavior and integration tests.
- High-frequency IPC streaming proves zero lock contention and bounded resource footprint.
