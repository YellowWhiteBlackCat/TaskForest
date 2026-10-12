# Handoff protocol

Args: `<abstract-socket-name-hex> <iface-index>`. The app binds a randomly
named abstract-namespace Unix socket (no filesystem path exists to create,
seize or leak); the launcher hex-decodes the name, connects, sends the fd,
and exits 0 after the app's one-byte ACK.

The ACK read is bounded (10 s): a receiver that dies mid-handoff gets a
typed `ack_failed` exit, never an immortal root process waiting on an ACK
that cannot come. The app side additionally admits only a uid-0 peer via
`SO_PEERCRED` before receiving (see `taskmanager-fd-bridge`).

## Verification

Authorization, helper identity, denial, timeout and descriptor handoff must
remain visible to the caller. Package/polkit checks and the live on-box
handoff receipt are separate; absence of the latter keeps the feature
partial.

```bash
cargo nextest run -p taskmanager-net-launcher -j 4 --all-targets
```

Related decisions: [ADR-023](../../../adr/023-per-feature-privilege-escalation-framework.md),
[ADR-024](../../../adr/024-afpacket-boundary-crate.md),
[ADR-025](../../../adr/025-fd-bridge-scm-rights-boundary-crate.md).
