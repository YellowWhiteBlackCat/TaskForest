# taskmanager-net-launcher

## Role

One-shot privileged Linux helper that opens AF_PACKET with CAP_NET_RAW and
passes an owned descriptor through SCM_RIGHTS (ADR-023/024/025).

## Boundary

The helper accepts a fixed, bounded argument vocabulary, performs no general
shell execution and emits a typed result. It does not become a daemon, grant
permanent capability or own application attribution.

## Contract and verification

Args: `<abstract-socket-name-hex> <iface-index>`. The app binds a randomly
named abstract-namespace Unix socket (no filesystem path exists to create,
seize or leak); the launcher hex-decodes the name, connects, sends the fd, and
exits 0 after the app's one-byte ACK. The ACK read is bounded (10 s): a
receiver that dies mid-handoff gets a typed `ack_failed` exit, never an
immortal root process waiting on an ACK that cannot come. The app side
additionally admits only a uid-0 peer via `SO_PEERCRED` before receiving
(see `taskmanager-fd-bridge`).

### Verification

Authorization, helper identity, denial, timeout and descriptor handoff must
remain visible to the caller. Package/polkit checks and the live on-box handoff
receipt are separate; absence of the latter keeps the feature partial.

## Module map

```text
src/main.rs   single-purpose network launcher helper
```
