# taskmanager-setup-helper

## Role

Fixed Linux first-run setup helper for the optional RAPL/udev integration.

## Boundary

Only the explicit install/revert vocabulary is accepted. The helper validates
the target, writes through bounded atomic steps, reloads the relevant subsystem
and rolls back on failure; it is not a general installer or shell.

## Contract and verification

The install/revert vocabulary, the typed outcome set and the fixture-versus-
receipt separation are specified in
[`docs/SETUP_ACTIONS.md`](docs/SETUP_ACTIONS.md).

## Module map

```text
src/main.rs   install/revert setup actions (udev rules, pkexec; idempotent,
              bounded atomic steps with rollback)
```

## Docs

- [`docs/SETUP_ACTIONS.md`](docs/SETUP_ACTIONS.md) — the install/revert vocabulary, atomic
  steps with rollback and verification.
- [ADR-040](../../adr/040-optional-setup-discovery.md) — optional setup discovery.
