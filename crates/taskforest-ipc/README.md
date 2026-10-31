# taskforest-ipc

High-performance compact binary IPC protocol and cross-platform transport for TaskForest Daemon.

## Role

Provides structured, bounded, and framed inter-process communication between the background `taskforest-daemon` service and multiple frontend client instances (`taskforest-g`, `taskforest-i`, `taskmanager-tui`, `taskforest-b`).

## Boundary

- **Safe Rust**: Strictly `#![forbid(unsafe_code)]`.
- **Channel Isolation**: Linux / macOS utilizes Unix Domain Sockets (`$XDG_RUNTIME_DIR/taskforest/daemon.sock`); Windows utilizes Named Pipes (`\\.\pipe\taskforest-daemon`).
- **Framing & DoS Protection**: Every message frame carries a magic prefix (`TFIP`), version byte, message opcode, monotonic sequence ID, bounded payload length (capped at 16 MiB), and CRC32 payload checksum.
- **On-Demand Sampling**: Clients explicitly declare their active subscription bitmask (e.g. CPU, Memory, Disk, Sensors); daemon throttles or sleeps unneeded polling facets when no client subscribes.

## Contract and verification

Wire framing integrity is guaranteed by `Frame::new` and validated by `Frame::from_parts`.
CRC32 verification protects against torn or corrupt socket buffers. The frame reader
enforces a 16 MiB ceiling against memory exhaustion.
Deterministic behavior is verified via headless integration tests covering UDS handshake,
bi-directional telemetry streaming, and high-frequency throughput.

## Module map

```text
src/
├── protocol/
│   ├── frame.rs      // Binary frame serialization, deserialization and CRC32 verification
│   ├── message.rs    // ClientMessage and DaemonMessage types
│   ├── mask.rs       // SubscriptionMask on-demand facet bitmask
│   └── error.rs      // Strongly-typed IPC protocol and transport errors
├── transport/
│   ├── framed.rs     // Bounded framed reader and writer
│   └── endpoint.rs   // Platform-specific socket/pipe path resolution
└── service/
    ├── client.rs     // Lightweight IPC client connector
    ├── server.rs     // Bounded daemon server listener
    └── session.rs    // Per-client session state and active subscription tracker
```
