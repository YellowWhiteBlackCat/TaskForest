# taskmanager-process-control-helper

## Role

One-shot privileged Unix/Windows helper for exactly one identity-checked
process-control operation per native authorization invocation.

## Boundary

It receives a fixed operation and frozen PID/start identity, revalidates before
the syscall and returns a typed result. It does not expose a shell, batch
control, blanket capability or UI state.

## Contract and verification

Success, target exit, PID reuse, permission, invalid operation and timeout must
remain distinguishable. Linux package staging/policy checks and Windows MSI
payload checks are separate from live installed authorization evidence.

## Module map

```text
src/main.rs   one identity-revalidated foreign-process operation
              (Windows UAC one-shot reply-file protocol, ADR-035)
```

## Docs

- [`docs/IDENTITY_REVALIDATION.md`](docs/IDENTITY_REVALIDATION.md) — the revalidation rule
  and distinguishable outcome vocabulary.
- [ADR-035](../../adr/035-windows-uac-foreign-process-control-transport.md) — UAC transport.
