# taskmanager-smbios-helper

## Role

One-shot Linux helper for SMBIOS memory-module and system/board identity
facts, invoked through the escalation seam and emitting a typed JSON envelope.

## Boundary

The helper performs exactly one operation: walk the DMI type-17 memory-device
entries plus the first type-0/1/2 identity entries through the shared SMBIOS
parser ([`docs/DMI_WALK.md`](docs/DMI_WALK.md)). No flags, no other path, no
general command running.

## Contract and verification

Exit codes, the field-disjoint SUCCESS/ERROR envelopes, null-for-unstated
fields, slot counting/sorting and denial classification are specified in
[`docs/DMI_WALK.md`](docs/DMI_WALK.md).

## Module map

```text
src/main.rs       root entry
src/dmi_walk.rs   read-only type-17 memory inventory walk (shared parser:
                  taskmanager-smbios-tables)
src/json.rs       fixed JSON envelope output
```

## Docs

- [`docs/DMI_WALK.md`](docs/DMI_WALK.md) — the type-17/0/1/2 walk, exit codes and fixture
  verification.
- [ADR-023](../../adr/023-per-feature-privilege-escalation-framework.md) — escalation
  framework.
