# taskmanager-afpacket

## Role

Audited AF_PACKET boundary for per-process network accounting. It is one of
the four allowed `unsafe` trust roots (ADR-024).

## Boundary

All socket creation, packet layout checks, bounded reads and ownership remain
inside this crate. Public APIs expose typed errors and owned `OwnedFd`/`AsFd`
values; raw pointers, handles and unbounded buffers never cross the boundary.

## Contract and verification

The crate is not a provider, privilege policy, or UI. CAP_NET_RAW acquisition
belongs to `taskmanager-net-launcher`; attribution and typed degradation stay
above it. Run its unit tests, boundary firewall and Miri gate before changing
the ABI surface.

## Module map

```text
src/lib.rs     AF_PACKET socket lifecycle and capability guard
src/parse.rs   frame parsing → typed per-process attribution
```

Without CAP_NET_RAW: honest degradation, no fabricated zeros.
