# Setup actions

Only the explicit install/revert vocabulary is accepted. The helper validates
the target, writes through bounded atomic steps, reloads the relevant
subsystem and rolls back on failure; it is not a general installer or shell.

`src/main.rs` implements the install/revert setup actions (udev rules,
pkexec; idempotent, bounded atomic steps with rollback).

## Verification

Conflict, permission, reload failure, retry, revert and restart outcomes are
typed. Test the helper in isolation and keep installed-package polkit and
restart receipts separate from fixture evidence.

```bash
cargo nextest run -p taskmanager-setup-helper -j 4 --all-targets
```

See also [ADR-040](../../../adr/040-optional-setup-discovery.md) for the optional
setup discovery decision and the escalation framework in
[ADR-023](../../../adr/023-per-feature-privilege-escalation-framework.md).
