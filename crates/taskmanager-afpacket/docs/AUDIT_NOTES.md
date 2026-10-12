# Audit notes

This crate is one of the four allowed `unsafe` trust roots (ADR-024). Its
scope rules (not a provider, no privilege policy, CAP_NET_RAW split) are the
crate boundary in the README.

## Verification

Run its unit tests, boundary firewall and Miri gate before changing the ABI
surface:

```bash
cargo nextest run -p taskmanager-afpacket -j 4
```