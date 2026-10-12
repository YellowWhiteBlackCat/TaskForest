# Audit boundary rules

The unsafe/handle/ownership containment rules are the crate boundary in the
README; this doc owns the query discipline and verification.

## Query discipline

Prefer safe crates first. Every query is bounded, checks reported lengths
and maps unsupported/permission/not-found to typed outcomes. Never add
PowerShell/CMD telemetry as a shortcut (ADR-018).

The capability inventory this boundary serves is listed in
the README Role section; the escalation `runas` call group is bounded per
ADR-035.

## Verification

Run Windows target contract, boundary firewall and native verification:

```bash
cargo check --locked -p taskmanager-windows-api --all-targets
cargo nextest run -p taskmanager-windows-api -j 4
```
