# taskmanager-net-launcher

## Role

One-shot privileged Linux helper that opens AF_PACKET with CAP_NET_RAW and
passes an owned descriptor through SCM_RIGHTS (ADR-023/024/025).

## Boundary

The helper accepts a fixed, bounded argument vocabulary, performs no general
shell execution and emits a typed result. It does not become a daemon, grant
permanent capability or own application attribution.

## Contract and verification

Args: `<abstract-socket-name-hex> <iface-index>`. Handoff is a bounded
one-byte-ACK protocol — no daemon, no permanent capability, no general shell
execution. The full protocol, the typed `ack_failed` rule and the uid-0
`SO_PEERCRED` admission are specified in
[`docs/HANDOFF_PROTOCOL.md`](docs/HANDOFF_PROTOCOL.md).

### Verification

Authorization, helper identity, denial, timeout and handoff visibility, plus
the package/polkit-versus-live-receipt separation, are specified in
[`docs/HANDOFF_PROTOCOL.md`](docs/HANDOFF_PROTOCOL.md).

## Module map

```text
src/main.rs   single-purpose network launcher helper
```

## Docs

- [`docs/HANDOFF_PROTOCOL.md`](docs/HANDOFF_PROTOCOL.md) — the abstract-socket fd handoff,
  bounded ACK and verification.
- [ADR-023](../../adr/023-per-feature-privilege-escalation-framework.md) /
  [ADR-024](../../adr/024-afpacket-boundary-crate.md) /
  [ADR-025](../../adr/025-fd-bridge-scm-rights-boundary-crate.md) — related boundary decisions.
